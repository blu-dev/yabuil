use std::str::Utf8Error;

use bevy::{
    asset::{AssetLoader, AsyncReadExt},
    math::Vec2,
    render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        texture::Image,
    },
};
use resvg::{
    tiny_skia::PixmapMut,
    usvg::{Options, TreeParsing},
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SvgLoaderError {
    #[error(transparent)]
    InvalidUtf8(#[from] Utf8Error),

    #[error(transparent)]
    SvgParseError(#[from] resvg::usvg::Error),

    #[error(transparent)]
    IO(#[from] std::io::Error),
}

#[derive(Default)]
pub struct SvgLoader;

impl AssetLoader for SvgLoader {
    type Asset = Image;
    type Error = SvgLoaderError;
    type Settings = ();

    fn extensions(&self) -> &[&str] {
        &["svg"]
    }

    fn load<'a>(
        &'a self,
        reader: &'a mut bevy::asset::io::Reader,
        _settings: &'a Self::Settings,
        _load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::utils::BoxedFuture<'a, Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let mut bytes = vec![];
            reader.read_to_end(&mut bytes).await?;

            let str = std::str::from_utf8(&bytes)?;

            let tree =
                resvg::Tree::from_usvg(&resvg::usvg::Tree::from_str(str, &Options::default())?);

            let size = Vec2::new(tree.size.width(), tree.size.height()).as_uvec2();

            let mut bytes = vec![0u8; (size.x * size.y * 4) as usize];

            let mut pixmap = PixmapMut::from_bytes(&mut bytes, size.x, size.y).unwrap();

            tree.render(Default::default(), &mut pixmap);

            let image = Image::new(
                Extent3d {
                    width: size.x,
                    height: size.y,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                bytes,
                TextureFormat::Rgba8UnormSrgb,
            );

            Ok(image)
        })
    }
}
