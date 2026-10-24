//! A game's own post-processing chain: the fullscreen passes its renderer
//! draws over every 3D frame with the game's post materials, in its order,
//! with the constants it sets before each one.

use crate::Unknown;

/// A code constant a post pass sets. The game's post materials read their
/// post controls and HDR controls from these rows; a row keeps the last
/// value a pass set until another pass sets it again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostConstant {
    /// Post control row `0..16`.
    Control(u8),
    /// HDR control row `0..2`.
    Hdr(u8),
}

/// A code texture a post material samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostSampler {
    /// Post texture `0..4`.
    Texture(u8),
    /// The resolved scene.
    ResolvedScene,
}

/// An image a post pass reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostImage {
    /// The resolved scene: the 3D frame as drawn, until a pass resolves the
    /// frame into it again.
    Scene,
    /// One of the chain's own render targets.
    Target(u8),
}

/// Where a post pass draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostOutput {
    /// One of the chain's own render targets.
    Target(u8),
    /// The frame buffer.
    Frame,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostFormat {
    /// Four 16-bit unsigned normalized channels.
    Rgba16Unorm,
    /// Four 8-bit unsigned normalized channels.
    Rgba8Unorm,
}

/// One of the chain's render targets: the display size shifted right by
/// `shift` (at least one pixel each way).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PostTarget {
    pub shift: u32,
    pub format: PostFormat,
}

impl PostTarget {
    #[must_use]
    pub fn size(&self, display: [u32; 2]) -> [u32; 2] {
        display.map(|side| (side >> self.shift).max(1))
    }
}

/// One fullscreen draw of a post material.
#[derive(Clone, Debug, PartialEq)]
pub struct PostPass {
    pub material: &'static str,
    pub output: PostOutput,
    /// The frame buffer is copied into the resolved scene before this pass.
    pub resolve_frame: bool,
    pub samplers: Vec<(PostSampler, PostImage)>,
    /// The rows this pass sets before it draws.
    pub constants: Vec<(PostConstant, [f32; 4])>,
}

/// What a frame's chain is planned from.
#[derive(Clone, Copy, Debug)]
pub struct PostFrame<'a> {
    /// The display size in pixels.
    pub display: [u32; 2],
    /// The vision the frame draws with, as the game's vision file reader
    /// leaves it.
    pub vision: &'a [u32],
}

/// How a game draws its post chain.
#[derive(Clone, Copy, Debug)]
pub struct PostChainRules {
    /// The chain's render targets, by [`PostImage::Target`] number.
    pub targets: &'static [PostTarget],
    /// Every material a frame's passes can draw.
    pub materials: &'static [&'static str],
    /// The frame's passes, in draw order.
    pub plan: fn(&PostFrame<'_>) -> Vec<PostPass>,
    /// The sampler state (filter and address modes) the passes read their
    /// code textures with.
    pub sampler_state: &'static Unknown,
    /// A client's vision channels: how the game picks and blends the vision
    /// each frame draws with from what its scripts ask for.
    pub vision_channels: fn() -> Box<dyn VisionChannels>,
}

/// Rules are told apart by their targets and materials: function addresses
/// are not stable to compare.
impl PartialEq for PostChainRules {
    fn eq(&self, other: &Self) -> bool {
        self.targets == other.targets
            && self.materials == other.materials
            && self.sampler_state == other.sampler_state
    }
}

/// A script call that changes the vision a client draws with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VisionCall {
    /// The server set the client's naked vision.
    ServerNaked { name: String, duration_ms: i32 },
    /// The server set the client's last-stand vision.
    ServerLastStand { name: String, duration_ms: i32 },
    /// A client script set the naked vision.
    ClientNaked { name: String, duration_ms: i32 },
    /// A client script turned the damage vision on or off.
    ClientDamage {
        on: bool,
        name: String,
        duration_ms: i32,
    },
    /// A client script set the underwater vision.
    ClientUnderwater { name: String, duration_ms: i32 },
}

/// The client state a frame's vision is picked by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VisionView {
    /// The client's time in milliseconds.
    pub now_ms: i32,
    /// The client's player is down in last stand.
    pub last_stand: bool,
}

/// The game's vision files, by path (`vision/<name>.vision`, lower case).
pub trait VisionFiles {
    fn text(&self, path: &str) -> Option<&str>;
}

/// One client's vision channels.
pub trait VisionChannels: Send + Sync {
    /// Takes a script call at the client's time `now_ms`.
    fn call(&mut self, call: &VisionCall, now_ms: i32, files: &dyn VisionFiles);
    /// The client's shellshock blends the naked vision to its own vision,
    /// or back to the server's naked vision (`None`), over `duration_ms`.
    fn shellshock(
        &mut self,
        vision: Option<&str>,
        duration_ms: i32,
        now_ms: i32,
        files: &dyn VisionFiles,
    );
    /// The vision this frame draws with, as the game's vision file reader
    /// leaves one.
    fn frame(&mut self, view: VisionView, files: &dyn VisionFiles) -> &[u32];
    /// What the game would have printed since the last call: files it could
    /// not open, fields it could not read.
    fn take_messages(&mut self) -> Vec<String>;
    /// The channel the last frame drew with and the name it holds.
    fn picked(&self) -> (usize, &str);
    /// The parts of the game's channel choice this port does not make yet.
    fn gaps(&self) -> &'static [&'static Unknown] {
        &[]
    }
}

/// A game's screen overlays: fullscreen draws of its own post materials over
/// the frame, after the renderer's post passes and before the HUD, each
/// reading the frame as drawn so far.
#[derive(Clone, Copy, Debug)]
pub struct OverlayRules {
    /// Every material the game's own overlays draw, drawn when the loaded
    /// zones carry them.
    pub materials: &'static [&'static str],
    /// The materials the game's client scripts map to its filter passes,
    /// drawn when the loaded zones carry them.
    pub filter_materials: &'static [&'static str],
    /// The frame's overlays, in draw order.
    pub plan: fn(&OverlayFrame<'_>) -> Vec<OverlayPass>,
    /// How the game blurs the frame for an overlay that asks for it.
    pub blur: Option<BlurRules>,
    /// What the game's overlay choice still leaves unknown.
    pub gaps: &'static [&'static Unknown],
}

/// A game's screen blur: images the frame's size shifted right by `shift`,
/// and the passes a radius takes over a frame of one size into images of
/// another.
#[derive(Clone, Copy, Debug)]
pub struct BlurRules {
    pub shift: u32,
    pub plan: fn(radius: f32, frame: [u32; 2], image: [u32; 2]) -> Vec<BlurPass>,
}

/// One pass of a screen blur: a filter material drawn over all of `target`
/// (`size`), reading `source` (as `feedback`), with eight filter tap rows.
#[derive(Clone, Debug, PartialEq)]
pub struct BlurPass {
    pub material: &'static str,
    pub source: BlurImage,
    pub target: BlurImage,
    pub size: [u32; 2],
    pub taps: [[f32; 4]; 8],
}

/// An image a blur reads or writes: the frame as the overlays found it,
/// one of two working images, or the blurred image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlurImage {
    Frame,
    Ping(u8),
    Blurred,
}

/// What an overlay draw reads as `feedback`: the frame as drawn so far, or
/// the image its blur left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OverlayFeedback {
    #[default]
    Frame,
    Blurred,
}

/// Rules are told apart by their materials: function addresses are not
/// stable to compare.
impl PartialEq for OverlayRules {
    fn eq(&self, other: &Self) -> bool {
        self.materials == other.materials && self.filter_materials == other.filter_materials
    }
}

/// What a frame's overlays are planned from.
#[derive(Clone, Copy, Debug)]
pub struct OverlayFrame<'a> {
    /// The client's time in milliseconds.
    pub now_ms: i32,
    /// The overlay state the client's game holds.
    pub screen: &'a ScreenOverlays,
    /// Each drawable material's texture atlas (rows, columns), by name.
    pub atlases: &'a [(&'static str, [u8; 2])],
    /// The scene's viewport the overlays draw over: x, y, width, height.
    pub viewport: [f32; 4],
    /// The client's script blur (`setblur`) as it stands this frame.
    pub script_blur: f32,
    /// The blur the menus shown over the world ask for this frame.
    pub menu_blur: f32,
    /// The `r_blur` dvar.
    pub r_blur: f32,
    /// The revive settings of the vision the client draws with.
    pub revive: Option<ReviveFx>,
}

/// A vision's revive settings (`r_reviveFX_*`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReviveFx {
    pub enable: bool,
    pub edge_color_temp: f32,
    pub edge_saturation: f32,
    pub edge_scale: [f32; 3],
    pub edge_contrast: [f32; 3],
    pub edge_offset: [f32; 3],
    pub edge_mask_adjust: f32,
    pub edge_amount: f32,
}

/// One fullscreen draw of an overlay material.
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayPass {
    pub material: String,
    /// The rows this draw sets before it draws.
    pub constants: Vec<(OverlayConstant, [f32; 4])>,
    /// The radius the frame is blurred with first (the blurred image is the
    /// game's first post effect texture), if any.
    pub blur: Option<f32>,
    /// What the draw reads as `feedback`.
    pub feedback: OverlayFeedback,
    /// The draw's vertex colour.
    pub color: [u8; 4],
}

/// A code row an overlay material reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayConstant {
    GameTime,
    FlameDistortion,
    OverlayTexCoord,
    FadeEffect,
    ViewportDimensions,
    /// The draw's target size: the draw puts its width and height in x and
    /// y, and keeps the planned z and w.
    RenderTargetSize,
    /// One of the four script vector rows, 0 to 3.
    ScriptVector(u8),
    /// One of the sixteen post control rows, 0 to 15.
    PostFxControl(u8),
}

/// The screen overlays a client's game keeps, as its scripts and server set
/// them, with the render settings its overlay draw reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScreenOverlays {
    /// The burning overlay, while it plays.
    pub burn: Option<FlameOverlay>,
    /// The electric shock overlay, while it plays.
    pub shock: Option<FlameOverlay>,
    /// The transporter overlay, while it plays.
    pub transporter: Option<FlameOverlay>,
    /// Whether flame overlays may draw at all.
    pub flame_allowed: bool,
    /// The flame overlays' fade override: 1 leaves the fade as it plays.
    pub flame_scaler: f32,
    /// The client scripts' filters that draw, in filter order.
    pub filters: Vec<ScreenFilter>,
    /// The poison overlay, while its amount is over 0.
    pub poison: Option<PoisonOverlay>,
    /// The revive settings `r_reviveFX_debug` sets in place of the
    /// vision's, while it is set.
    pub revive_debug: Option<ReviveFx>,
}

/// The poison overlay: its amount and the render settings its draw reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoisonOverlay {
    pub amount: f32,
    /// The distortion's pulse rate.
    pub pulse: f32,
    /// The double vision's horizontal offset.
    pub dvision_x: f32,
    /// The blur it adds, from `blur_min` to `blur_max`.
    pub blur_min: f32,
    pub blur_max: f32,
}

/// A client script filter that draws: its first pass's material and the
/// sixteen constants the scripts set for that pass.
#[derive(Clone, Debug, PartialEq)]
pub struct ScreenFilter {
    /// The filter, 0 to 7.
    pub filter: u8,
    pub material: String,
    pub constants: [f32; 16],
}

/// A flame-style overlay (burning, electric shock, transporter): the
/// settings it took when it began, and its fade once it ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlameOverlay {
    /// The distortion scale (horizontal, vertical).
    pub distortion: [f32; 2],
    pub magnitude: f32,
    /// The texture atlas frames per second.
    pub fps: f32,
    /// When the fade began (the client's time in milliseconds).
    pub fade_start_ms: i32,
    /// How long the fade lasts; 0 while the overlay has not ended.
    pub fade_ms: i32,
}
