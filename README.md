# yabuil
(yah-bwee-ll)
Yet-Another-Bevy-UI-Library

(Maybe there aren't enough of these yet to warrant calling it this, but I don't have any other name...)

## The Goal
yabuil's goal is to provide a simple-to-understand UI development experience based on Bevy's ECS engine.

It does this by defining an asset type with the extension `layout.json` with the following structure:
```json
{
    "resolution": [1920.0, 1080.0],
    "canvas_size": [1920.0, 1080.0],
    "nodes": [
        {
            "id": "background_image",
            "position": [0.0, 0.0],
            "size": [1920.0, 1080.0],
            "anchor": "TopLeft",
            "node_kind": "Image",
            "node_data": {
                "path": "images/rivals_background.png"
            },
            "attributes": {
                "NearestNeighbor": {},
                "ImageTint": {
                    "color": [1.0, 1.0, 1.0, 0.4]
                }
            }
        }
    ],
    "animations": {
        "slide_in": [
            {
                "id": "background_image",
                "time_ms": 100.0,
                "target": {
                    "Position": {
                        "start": [-300.0, -300.0],
                        "end": [0.0, 0.0]
                    }
                }
            }
        ]
    }
}
```

## Layouts
Layouts are a collection of UI nodes, where a node can be one of the following primitives:
- `Null` - Completely user-defined appearance/representation, the layout engine provides animation/metadata propagation for these nodes
- `Image` - Node is spawned with a `SpriteBundle` with the image provided as a path
- `Text` - Node is spawned with a `Text2dBundle`
- `Layout` - Node is spawned as a sublayout with the layout to spawn in provided by path (more on this later)

Layouts also have a `resolution`, which defines the unit/scale to interpret the coordinates of positions/sizes in. There is no relative functionality built into yabuil by default. The entire UI is scaled in proportion with the size of the render target which the layout is parented to: `layout_scale = render_target.size() / layout.resolution()`.

The `resolution` field in the layout is optional, and will default to the `canvas_size` field (non-optional) when it is not present.

The `canvas_size` field is how much space (according to the resolution) that a layout should take up. This is important when using layouts as sub-layouts.

Nodes also have attributes! This is the most important feature of yabuil, as there are only a few attributes provided built-in to yabuil (more welcome in PRs, of course).

## Node Attributes

Every node in a yabuil layout can specify attributes that integrate very closely to the ECS/user code to customize the behavior of the layout. This is powered by the `LayoutAttribute` trait:
```rs
pub trait LayoutAttribute: Send + Sync + 'static {
    fn apply(&self, world: &mut NodeWorldViewMut);
    fn initialize_dependencies(&mut self, context: &mut RestrictedLoadContext) {}
    fn visit_dependencies(&self, visit_fn: &mut dyn FnMut(UntypedAssetId)) {}
}
```

A note about when these are called:
- `LayoutAttribute::apply` is called when a `yabuil::Layout` is spawned into the ECS
- `LayoutAttribute::initialize_dependencies` is called during the loading of a `yabuil::Layout`
    - This method should be used to load assets that your attribute depends on. These will be tracked by the `VisitAssetDepencies`
    implementation of `yabuil::Layout` (as long as you `visit_dependencies` is also implemented) so that their `RecursiveDependencyLoadState` reflects all of the attributes as well
    - Look at the `bevy_menu` example's `CustomImage` attribute
- `LayoutAttribute::visit_dependencies` is called during the `VisitAssetDependencies` impl of `yabuil::Layout` to track an attributes dependencies

Attributes are deserialized and processed *during* the `AssetLoader::load` implementation for `yabuil::Layout`s. They are deserialized using a custom, manually implement `serde::de::DeserializeSeed` implementation that takes a reference to the `AttributeRegistry` as context. This makes it noteworthy that `initialize_dependencies` and `visit_dependencies` are only called on the asset load, and `apply` is used on every node that is spawned with that attribute (this happens a lot when composing layouts).

## Registering an Attribute
Suppose you define an attribute that will apply a tint to an image node when the `yabuil::Layout` gets loaded into the ECS, like this:

```rs
#[derive(Deserialize, Serialize)]
pub struct ImageTint {
    color: [f32; 4]
}

impl yabuil::LayoutAttribute for ImageTint {
    fn apply(&self, world: &mut NodeWorldViewMut) {
        world
            .as_image_node_mut()
            .expect("ImageTint attribute should only be used on image nodes")
            .update_sprite(|sprite| {
                sprite.color = Color::rgba(self.color[0], self.color[1], self.color[2], self.color[3]);
            });
    }
}
```

In order for to be properly applied to layouts, you need to register it with the `AttributeRegistry`, which can be done easily with the `yabuil::LayoutApp` extension trait:
```rs
pub fn main() {
    App::new()
        .add_plugins((DefaultPlugins, yabuil::LayoutPlugin))
        .register_layout_attribute::<ImageTint>("ImageTint")
        .run();
}
```

## Layout Node Views
yabuil provides wrappers around bevy's ECS entity access types to provide easy node layout tree traversal:
- `ImageNodeView` - Provides read-only access to image node data like the texture handle/sprite component
- `ImageNodeViewMut` - Provides mutable access to the same things
- `TextNodeView` - Provides read-only access to text node data like the text and styling
- `TextNodeViewMut` - Provides mutable access to the same things
- `LayoutNodeView` - Provides read-only methods to look at layout animations
- `LayoutNodeViewMut` - Provides the ability to play animations on a layout
- `NodeView` - Provides read-only access to metadata and read-only conversion methods into other, less-powerful node views
- `NodeViewMut` - Provided mutable access to metadata and mutable conversion methods inot other, less-powerful node views
- `NodeWorldView` - Provides read-only access to metadata and read-only access methods for parent nodes, child nodes, and sibling nodes. Also provides the same read-only conversion methods as `NodeView`
- `NodeWorldViewMut` - Provides mutable access to metadata and scoped mutable access methods for parent, child, and sibling nodes. Also provides the same mutable conversion methods as `NodeViewMut`

You should use these when they are given to you by `yabuil`, and you can create `NodeView(Mut)` and `NodeWorldView(Mut)` from their `new` constructors which check to ensure they are a layout node before providing the access.

## Layout Animations
yabuil provides an animation implementation that is currently somewhat limited but will eventually follow the same registry-style as attributes.

Animations are keyed by their name (a `String`) and can be used to update any node(s) in the layout, including multiple actions on the same node. This is done by chaining together `NodeAnimations`, which have a:
- `id` - The node's ID
- `time_ms` - The number of milliseconds the animation should play for
- `target` - What to actually animate

## Putting it all together
I'd recommend running the `main_menu` and `rivals` examples to see how these things can work together. Some notes about the examples:
- `main_menu` was put together iteratively as I implemented new features, hence why the colors are updated via code instead of via the `animations` field.
- `rivals` was put together in about 3 hours, including the time it took to troubleshoot text not lining up properly AND dumping all of the resources used from Rivals of Aether.

# License
This project and all CODE is licensed under MIT or Apache-2.0 at your choice. The assets are not included and belong to the [Bevy Engine project](https://github.com/bevyengine) and Aether Studios/Dan Fornace.