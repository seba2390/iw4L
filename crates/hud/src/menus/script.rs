#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Command {
    Open(String),
    Close(String),
    Escape(String),
    SetFocus(String),
    FocusFirst,
    SetItemColor {
        item: String,
        field: String,
        color: [f32; 4],
    },
    Show(String),
    Hide(String),
    /// `showMenu`/`hideMenu`: a whole menu, when one of that name is loaded.
    ShowMenu(String),
    HideMenu(String),
    /// `setLocalVarInt`/`Bool`/`Float`/`String name value`.
    SetLocalVar {
        kind: LocalVarKind,
        name: String,
        value: String,
    },
    /// Black Ops zombies' `nosavehide item` / `saveAvailableHide item`: the
    /// item shows only while a saved game is (or, for the second, is not)
    /// available.
    SaveVisibility {
        item: String,
        hide_when_available: bool,
    },
    Play(String),
    ScriptMenuResponse(String),
    Exec(String),
    SetDvar(String, String),
    CopyDvar(String, String),
    ExecOnDvarIntValue {
        dvar: String,
        value: i32,
        command: String,
    },
    ExecOnDvarStringValue {
        dvar: String,
        value: String,
        command: String,
    },
    OpenForGameType(String),
    CloseForGameType(String),
    SetFocusByDvar(String),
    MenuOnDvar {
        open: bool,
        want_match: bool,
        dvar: String,
        value: String,
        menu: String,
    },
    /// Black Ops' `uiScript sendMenuNotify "name arg"`: a message to the
    /// level's scripts.
    SendMenuNotify(String),
    Unhandled(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalVarKind {
    Bool,
    Int,
    Float,
    String,
}

pub(crate) fn tokens(script: &str) -> Vec<String> {
    spanned_tokens(script)
        .into_iter()
        .map(|(token, _)| token)
        .collect()
}

/// A console line's commands, split at each `;` outside quotes.
pub(crate) fn statements(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut quoted = false;
    let mut start = 0;
    for (at, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ';' | '\n' if !quoted => {
                out.push(line[start..at].trim().to_owned());
                start = at + 1;
            }
            _ => {}
        }
    }
    out.push(line[start..].trim().to_owned());
    out.retain(|statement| !statement.is_empty());
    out
}

/// Each token with the byte offset just past it in `script`.
fn spanned_tokens(script: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let mut chars = script.char_indices().peekable();
    while let Some(&(_, c)) = chars.peek() {
        if c.is_whitespace() || c == ';' {
            chars.next();
            continue;
        }
        let mut token = String::new();
        let mut end = script.len();
        if c == '"' {
            chars.next();
            for (at, c) in chars.by_ref() {
                if c == '"' {
                    end = at + 1;
                    break;
                }
                token.push(c);
            }
        } else {
            while let Some(&(at, c)) = chars.peek() {
                if c.is_whitespace() || c == ';' || c == '"' {
                    end = at;
                    break;
                }
                token.push(c);
                chars.next();
            }
        }
        out.push((token, end));
    }
    fold_expression_args(out)
}

/// Marks a command argument Black Ops evaluates when the command runs: a
/// function of the menu expressions, `name ( args )`, folded to one token.
pub(crate) const EXPRESSION_ARG: char = '\u{1}';

fn fold_expression_args(tokens: Vec<(String, usize)>) -> Vec<(String, usize)> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let call = tokens[i]
            .0
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
            && tokens.get(i + 1).is_some_and(|(t, _)| t == "(");
        let close = call
            .then(|| tokens[i + 2..].iter().position(|(t, _)| t == ")"))
            .flatten();
        match close {
            Some(close) => {
                let args: Vec<&str> = tokens[i + 2..i + 2 + close]
                    .iter()
                    .map(|(t, _)| t.as_str())
                    .collect();
                let end = tokens[i + 2 + close].1;
                out.push((
                    format!("{EXPRESSION_ARG}{}({})", tokens[i].0, args.join(" ")),
                    end,
                ));
                i += close + 3;
            }
            None => {
                out.push(tokens[i].clone());
                i += 1;
            }
        }
    }
    out
}

/// Black Ops' `exec` / `execNow` argument: the rest of the statement, up to
/// the first `;` outside quotes (an escaped `\"` is no quote), with the
/// quotes taken off when one pair encloses it all. Returns the command and
/// the byte offset past what it read.
fn statement_rest(script: &str, from: usize) -> (String, usize) {
    let mut text: Vec<char> = Vec::new();
    let (mut quotes, mut first_quote, mut last_quote) = (0, None, None);
    let (mut first_solid, mut last_solid) = (None, None);
    let mut end = script.len();
    let mut prev = script[..from].chars().next_back();
    for (at, c) in script[from..].char_indices() {
        let index = text.len();
        text.push(c);
        if c == '"' && prev != Some('\\') {
            quotes += 1;
            first_quote.get_or_insert(index);
            last_quote = Some(index);
        }
        if c != ' ' && c != '\t' {
            first_solid.get_or_insert(index);
            if c != ';' {
                last_solid = Some(index);
            }
        }
        prev = Some(c);
        if c == ';' && quotes % 2 == 0 {
            end = from + at + 1;
            break;
        }
    }
    if quotes == 2
        && let (Some(open), Some(close)) = (first_quote, last_quote)
        && first_solid == Some(open)
        && last_solid == Some(close)
    {
        text[open] = ' ';
        text[close] = ' ';
    }
    let text: String = text.into_iter().collect();
    let text = text.trim();
    (
        text.strip_suffix(';').unwrap_or(text).trim().to_owned(),
        end,
    )
}

/// A script's commands. With `exec_takes_statement` (Black Ops), `exec` and
/// `execNow` run the rest of their statement; otherwise one token.
pub(crate) fn parse(script: &str, exec_takes_statement: bool) -> Vec<Command> {
    let spanned = spanned_tokens(script);
    let toks: Vec<String> = spanned.iter().map(|(t, _)| t.clone()).collect();
    let mut out = Vec::new();
    let mut i = 0;
    let arg = |i: usize| toks.get(i).cloned().unwrap_or_else(String::new);
    let float = |i: usize| {
        toks.get(i)
            .and_then(|t| t.parse::<f32>().ok())
            .unwrap_or(0.0)
    };
    while i < toks.len() {
        let name = toks[i].to_ascii_lowercase();
        if exec_takes_statement && (name == "exec" || name == "execnow") {
            let (text, end) = statement_rest(script, spanned[i].1);
            out.push(Command::Exec(text));
            i += 1;
            while spanned.get(i).is_some_and(|(_, at)| *at <= end) {
                i += 1;
            }
            continue;
        }
        let (command, used) = match name.as_str() {
            "open" | "ingameopen" => (Command::Open(arg(i + 1)), 2),
            "close" | "ingameclose" => (Command::Close(arg(i + 1)), 2),
            "escape" => (Command::Escape(arg(i + 1)), 2),
            "setfocus" => (Command::SetFocus(arg(i + 1)), 2),
            "focusfirst" => (Command::FocusFirst, 1),
            "setitemcolor" => (
                Command::SetItemColor {
                    item: arg(i + 1),
                    field: arg(i + 2).to_ascii_lowercase(),
                    color: [float(i + 3), float(i + 4), float(i + 5), float(i + 6)],
                },
                7,
            ),
            "show" => (Command::Show(arg(i + 1)), 2),
            "hide" => (Command::Hide(arg(i + 1)), 2),
            "showmenu" => (Command::ShowMenu(arg(i + 1)), 2),
            "hidemenu" => (Command::HideMenu(arg(i + 1)), 2),
            "setlocalvarbool" | "setlocalvarint" | "setlocalvarfloat" | "setlocalvarstring" => {
                let kind = match name.as_str() {
                    "setlocalvarbool" => LocalVarKind::Bool,
                    "setlocalvarint" => LocalVarKind::Int,
                    "setlocalvarfloat" => LocalVarKind::Float,
                    _ => LocalVarKind::String,
                };
                (
                    Command::SetLocalVar {
                        kind,
                        name: arg(i + 1),
                        value: arg(i + 2),
                    },
                    3,
                )
            }
            "nosavehide" | "saveavailablehide" => (
                Command::SaveVisibility {
                    item: arg(i + 1),
                    hide_when_available: name == "saveavailablehide",
                },
                2,
            ),
            "play" => (Command::Play(arg(i + 1)), 2),
            "scriptmenuresponse" => (Command::ScriptMenuResponse(arg(i + 1)), 2),
            "exec" | "execnow" => (Command::Exec(arg(i + 1)), 2),
            "copydvar" => (Command::CopyDvar(arg(i + 1), arg(i + 2)), 3),
            "setdvar" => (Command::SetDvar(arg(i + 1), arg(i + 2)), 3),
            "execondvarintvalue" | "execnowondvarintvalue" => (
                Command::ExecOnDvarIntValue {
                    dvar: arg(i + 1),
                    value: arg(i + 2).parse().unwrap_or(0),
                    command: arg(i + 3),
                },
                4,
            ),
            "execondvarstringvalue" => (
                Command::ExecOnDvarStringValue {
                    dvar: arg(i + 1),
                    value: arg(i + 2),
                    command: arg(i + 3),
                },
                4,
            ),
            "setfocusbydvar" => (Command::SetFocusByDvar(arg(i + 1)), 2),
            "openforgametype" => (Command::OpenForGameType(arg(i + 1)), 2),
            "closeforgametype" => (Command::CloseForGameType(arg(i + 1)), 2),
            "uiscript" => {
                let script = arg(i + 1).to_ascii_lowercase();
                match script.as_str() {
                    "quit" => (Command::Exec("quit".into()), 2),
                    "sendmenunotify" => (Command::SendMenuNotify(arg(i + 2)), 3),
                    "openmenuondvar" | "openmenuondvarnot" | "closemenuondvar"
                    | "closemenuondvarnot" => (
                        Command::MenuOnDvar {
                            open: script.starts_with("open"),
                            want_match: !script.ends_with("not"),
                            dvar: arg(i + 2),
                            value: arg(i + 3),
                            menu: arg(i + 4),
                        },
                        5,
                    ),
                    _ => (Command::Unhandled(format!("uiscript {script}")), 2),
                }
            }
            "fadein" | "fadeout" | "setbackground" | "playlooped" => {
                (Command::Unhandled(format!("{name} {}", arg(i + 1))), 2)
            }
            "setcolor" => (Command::Unhandled(name.clone()), 6),
            _ => (Command::Unhandled(name.clone()), 1),
        };
        out.push(command);
        i += used;
    }
    out
}
