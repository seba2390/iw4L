//! A sink that hands some of its hooks to a sink it holds, as a zone walk
//! hands its sound hooks to the sound capture it carries.

use super::AssetLinkSink;
use crate::zone::Result;

/// What a forwarding sink holds: the sink its hooks go to, while there is
/// one, and what becomes of a hook's result. The walk itself goes on.
pub trait LinkForward {
    type Sink: AssetLinkSink;

    fn sink(&mut self) -> Option<&mut Self::Sink>;

    fn keep(&mut self, result: Result<()>);
}

impl<T: LinkForward> LinkForward for Option<T> {
    type Sink = T::Sink;

    fn sink(&mut self) -> Option<&mut T::Sink> {
        self.as_mut()?.sink()
    }

    fn keep(&mut self, result: Result<()>) {
        if let Some(holder) = self {
            holder.keep(result);
        }
    }
}

pub fn forward_link<H: LinkForward>(
    holder: &mut H,
    hook: impl FnOnce(&mut H::Sink) -> Result<()>,
) -> Result<()> {
    if let Some(sink) = holder.sink() {
        let result = hook(sink);
        holder.keep(result);
    }
    Ok(())
}

/// The sound hooks of [`AssetLinkSink`], written inside an `impl` of it:
/// each goes to the [`LinkForward`] in the field named.
#[macro_export]
macro_rules! forward_sound_links {
    ($field:ident) => {
        fn capture_snd_rows(
            &mut self,
            s: &$crate::ZoneStream<'_>,
            kind: $crate::SndRows,
            rows: $crate::Ptr,
            count: usize,
        ) -> $crate::Result<()> {
            $crate::forward_link(&mut self.$field, |sink| {
                $crate::AssetLinkSink::capture_snd_rows(sink, s, kind, rows, count)
            })
        }

        fn capture_loaded_sound(
            &mut self,
            s: &$crate::ZoneStream<'_>,
            header: $crate::Ptr,
            pcm: $crate::Ptr,
            data_len: usize,
        ) -> $crate::Result<()> {
            $crate::forward_link(&mut self.$field, |sink| {
                $crate::AssetLinkSink::capture_loaded_sound(sink, s, header, pcm, data_len)
            })
        }

        fn bind_last_loaded_to_sound_file(&mut self, file: $crate::Ptr) -> $crate::Result<()> {
            $crate::forward_link(&mut self.$field, |sink| {
                $crate::AssetLinkSink::bind_last_loaded_to_sound_file(sink, file)
            })
        }

        fn capture_sound(
            &mut self,
            s: &$crate::ZoneStream<'_>,
            list: $crate::Ptr,
            count: usize,
            head: Option<$crate::Ptr>,
        ) -> $crate::Result<()> {
            $crate::forward_link(&mut self.$field, |sink| {
                $crate::AssetLinkSink::capture_sound(sink, s, list, count, head)
            })
        }

        fn bind_streamed_sound_file(
            &mut self,
            file: $crate::Ptr,
            dir: &str,
            name: &str,
        ) -> $crate::Result<()> {
            $crate::forward_link(&mut self.$field, |sink| {
                $crate::AssetLinkSink::bind_streamed_sound_file(sink, file, dir, name)
            })
        }
    };
}

/// The nested shader and vertex declaration hooks of [`AssetLinkSink`],
/// written inside an `impl` of it: each goes to the sink in the field named.
#[macro_export]
macro_rules! forward_nested_links {
    ($field:ident) => {
        fn nested_shader(
            &mut self,
            s: &$crate::ZoneStream<'_>,
            kind: $crate::NestedShaderKind,
            slot: $crate::Ptr,
        ) -> $crate::Result<()> {
            $crate::AssetLinkSink::nested_shader(&mut self.$field, s, kind, slot)
        }

        fn nested_shader_alias(
            &mut self,
            kind: $crate::NestedShaderKind,
            slot: $crate::Ptr,
            target: $crate::Ptr,
        ) -> $crate::Result<()> {
            $crate::AssetLinkSink::nested_shader_alias(&mut self.$field, kind, slot, target)
        }

        fn nested_vertex_decl(
            &mut self,
            s: &$crate::ZoneStream<'_>,
            slot: $crate::Ptr,
        ) -> $crate::Result<()> {
            $crate::AssetLinkSink::nested_vertex_decl(&mut self.$field, s, slot)
        }

        fn nested_vertex_decl_alias(
            &mut self,
            slot: $crate::Ptr,
            target: $crate::Ptr,
        ) -> $crate::Result<()> {
            $crate::AssetLinkSink::nested_vertex_decl_alias(&mut self.$field, slot, target)
        }
    };
}

/// The material name hooks of [`AssetLinkSink`], written inside an `impl`
/// of it: they keep and answer the [`crate::LinkNames`] in the field named.
#[macro_export]
macro_rules! forward_material_names {
    ($field:ident) => {
        fn remember_material_name(
            &mut self,
            slot: $crate::Ptr,
            insert_slot: Option<$crate::Ptr>,
            name: $crate::Ptr,
        ) {
            self.$field.remember(slot, insert_slot, name);
        }

        fn material_name_ptr(&self, slot: $crate::Ptr) -> Option<$crate::Ptr> {
            self.$field.get(slot)
        }
    };
}

/// The model name hooks of [`AssetLinkSink`], as
/// [`forward_material_names`] writes the material ones.
#[macro_export]
macro_rules! forward_xmodel_names {
    ($field:ident) => {
        fn remember_xmodel_name(
            &mut self,
            slot: $crate::Ptr,
            insert_slot: Option<$crate::Ptr>,
            name: $crate::Ptr,
        ) {
            self.$field.remember(slot, insert_slot, name);
        }

        fn xmodel_name_ptr(&self, slot: $crate::Ptr) -> Option<$crate::Ptr> {
            self.$field.get(slot)
        }
    };
}
