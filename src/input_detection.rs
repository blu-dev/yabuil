use bevy::{
    ecs::system::{EntityCommand, SystemParam},
    prelude::*,
    render::camera::RenderTarget,
    utils::HashMap,
    window::{PrimaryWindow, WindowRef},
};
use serde::{Deserialize, Serialize};

use crate::{
    components::RootNode,
    node::{ComputedBoundingBox, LayoutInfo},
    views::NodeWorldViewMut,
    ActiveLayout, LayoutAttribute, LayoutId,
};

const fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Reflect)]
pub struct InputDetection {
    #[serde(default = "default_true")]
    use_camera_window: bool,
}

#[derive(Hash, PartialEq, Eq, Copy, Clone)]
pub enum Cursor {
    CameraWindow,
    Custom(Entity),
}

#[derive(Component, Default, Deref, DerefMut)]
pub struct LayoutCursors(Vec<Cursor>);

#[derive(Component)]
pub struct LayoutCursorPosition {
    pub position: Vec2,
    pub left_click: bool,
    pub right_click: bool,
    pub middle_click: bool,
}

#[derive(Default, Copy, Clone, PartialEq, Eq)]
struct InputDetectionState {
    is_hover: bool,
    is_left: bool,
    is_right: bool,
    is_middle: bool,
}

#[derive(Default, Copy, Clone, PartialEq, Eq)]
struct GlobalInputDetectionState {
    hover_count: usize,
    left_count: usize,
    right_count: usize,
    middle_count: usize,
}

impl GlobalInputDetectionState {
    fn inc_hover(&mut self) -> bool {
        self.hover_count += 1;
        self.hover_count == 1
    }

    fn dec_hover(&mut self) -> bool {
        self.hover_count = self.hover_count.saturating_sub(1);
        self.hover_count == 0
    }

    fn inc_left(&mut self) -> bool {
        self.left_count += 1;
        self.left_count == 1
    }

    fn dec_left(&mut self) -> bool {
        self.left_count = self.left_count.saturating_sub(1);
        self.left_count == 0
    }
    fn inc_right(&mut self) -> bool {
        self.right_count += 1;
        self.right_count == 1
    }

    fn dec_right(&mut self) -> bool {
        self.right_count = self.right_count.saturating_sub(1);
        self.right_count == 0
    }
    fn inc_middle(&mut self) -> bool {
        self.middle_count += 1;
        self.middle_count == 1
    }

    fn dec_middle(&mut self) -> bool {
        self.middle_count = self.middle_count.saturating_sub(1);
        self.middle_count == 0
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum EventKind {
    Click,
    Unclick,
    RightClick,
    RightUnclick,
    MiddleClick,
    MiddleUnclick,
    Hover,
    Unhover,
}

struct CallEventHandlerCommand {
    event: EventKind,
    cursor: Cursor,
}

impl CallEventHandlerCommand {
    fn new(event: EventKind, cursor: Cursor) -> Self {
        Self { event, cursor }
    }
}

struct CallGlobalEventHandlerCommand(EventKind);

macro_rules! call_event_handlers {
    ($event:expr, $state:ident, $cursor:expr, $node:ident; $($kind:ident => $field:ident),*) => {
        match $event {
            $(
                EventKind::$kind => {
                    for callback in $state.$field.iter_mut() {
                        (callback)($event, $cursor, &mut $node);
                    }
                }
            )*
        }
    };
    (global $event:expr, $state:ident, $node:ident; $($kind:ident => $field:ident),*) => {
        match $event {
            $(
                EventKind::$kind => {
                    for callback in $state.$field.iter_mut() {
                        (callback)($event, &mut $node);
                    }
                }
            )*
        }
    };
}

impl EntityCommand for CallGlobalEventHandlerCommand {
    fn apply(self, id: Entity, world: &mut World) {
        let Some(mut node) = NodeWorldViewMut::new(world.entity_mut(id)) else {
            log::error!("Input detection event sent for entity which is not a node");
            return;
        };

        let mut state = std::mem::take(
            &mut *node
                .as_entity_mut()
                .get_mut::<LayoutNodeInputDetection>()
                .unwrap(),
        );
        call_event_handlers!(
            global self.0, state, node;
            Click => on_global_click,
            RightClick => on_global_right_click,
            MiddleClick => on_global_middle_click,
            Hover => on_global_hover,
            Unclick => on_global_unclick,
            RightUnclick => on_global_right_unclick,
            MiddleUnclick => on_global_middle_unclick,
            Unhover => on_global_unhover
        );

        *node
            .as_entity_mut()
            .get_mut::<LayoutNodeInputDetection>()
            .unwrap() = state;
    }
}

impl EntityCommand for CallEventHandlerCommand {
    fn apply(self, id: Entity, world: &mut World) {
        let Some(mut node) = NodeWorldViewMut::new(world.entity_mut(id)) else {
            log::error!("Input detection event sent for entity which is not a node");
            return;
        };

        let mut state = std::mem::take(
            &mut *node
                .as_entity_mut()
                .get_mut::<LayoutNodeInputDetection>()
                .unwrap(),
        );
        call_event_handlers!(
            self.event, state, self.cursor, node;
            Click => on_click,
            RightClick => on_right_click,
            MiddleClick => on_middle_click,
            Hover => on_hover,
            Unclick => on_unclick,
            RightUnclick => on_right_unclick,
            MiddleUnclick => on_middle_unclick,
            Unhover => on_unhover
        );

        *node
            .as_entity_mut()
            .get_mut::<LayoutNodeInputDetection>()
            .unwrap() = state;
    }
}

type EventHandlerList =
    Vec<Box<dyn FnMut(EventKind, Cursor, &mut NodeWorldViewMut) + Send + Sync + 'static>>;

type GlobalEventHandlerList =
    Vec<Box<dyn FnMut(EventKind, &mut NodeWorldViewMut) + Send + Sync + 'static>>;

macro_rules! decl_event_handlers {
    (global { $($global_name:ident),* }; specific { $($name:ident),* }) => {
        #[derive(Component, Default)]
        pub struct LayoutNodeInputDetection {
            global_state: GlobalInputDetectionState,
            state: HashMap<Cursor, InputDetectionState>,
            $(
                $global_name: GlobalEventHandlerList,
            )*
            $(
                $name: EventHandlerList,
            )*
        }

        impl LayoutNodeInputDetection {
            $(
                pub fn $global_name(&mut self, f: impl FnMut(EventKind, &mut NodeWorldViewMut) + Send + Sync + 'static) {
                    self.$global_name.push(Box::new(f));
                }
            )*

            $(
                pub fn $name(&mut self, f: impl FnMut(EventKind, Cursor, &mut NodeWorldViewMut) + Send + Sync + 'static) {
                    self.$name.push(Box::new(f));
                }
            )*
        }
    }
}

decl_event_handlers!(
    global {
        on_global_click,
        on_global_right_click,
        on_global_middle_click,
        on_global_hover,
        on_global_unclick,
        on_global_right_unclick,
        on_global_middle_unclick,
        on_global_unhover
    };
    specific {
        on_click,
        on_right_click,
        on_middle_click,
        on_hover,
        on_unclick,
        on_right_unclick,
        on_middle_unclick,
        on_unhover
    }
);

impl LayoutAttribute for InputDetection {
    fn apply(&self, world: &mut NodeWorldViewMut) {
        let world = world.as_entity_world_mut();

        world.insert((
            LayoutNodeInputDetection::default(),
            ComputedBoundingBox::default(),
        ));

        let mut cameras = LayoutCursors::default();

        if self.use_camera_window {
            cameras.push(Cursor::CameraWindow);
        }

        world.insert(cameras);
    }

    fn revert(&self, world: &mut NodeWorldViewMut) {
        world
            .as_entity_world_mut()
            .remove::<(LayoutNodeInputDetection, ComputedBoundingBox)>();
    }
}

#[derive(SystemParam)]
pub(crate) struct UpdateInputDetectionState<'w, 's> {
    windows: Query<'w, 's, &'static Window>,
    primary_window: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    roots: Query<
        'w,
        's,
        (
            &'static Parent,
            &'static GlobalTransform,
            &'static LayoutInfo,
        ),
        (With<RootNode>, With<ActiveLayout>),
    >,
    cameras: Query<'w, 's, &'static Camera>,
    cursor_positions: Local<'s, HashMap<Entity, Option<Vec2>>>,
}

impl<'w, 's> UpdateInputDetectionState<'w, 's> {
    fn get_camera_cursors_for_layout(&mut self, layout: LayoutId) -> Option<Vec2> {
        let layout_id = layout.0;
        if let Some(cursor) = self.cursor_positions.get(&layout_id) {
            return *cursor;
        }

        let Ok((layout, _, _)) = self.roots.get(layout_id) else {
            log::warn!("Failed to get layout with id {layout_id:?}");
            return None;
        };

        let Ok(camera) = self.cameras.get(layout.get()) else {
            log::warn!("Layout {layout_id:?} is not the direct child of a camera");
            return None;
        };

        match &camera.target {
            RenderTarget::Window(WindowRef::Primary) => {
                let Ok(window) = self.primary_window.get_single() else {
                    log::warn!("Failed to get primary window");
                    return None;
                };

                let cursor = window.cursor_position();

                self.cursor_positions
                    .insert(layout_id, window.cursor_position());
                return cursor;
            }
            RenderTarget::Window(WindowRef::Entity(entity)) => {
                let Ok(window) = self.windows.get(*entity) else {
                    log::warn!("Failed to get window {entity:?}");
                    return None;
                };

                let cursor = window.cursor_position();

                self.cursor_positions
                    .insert(layout_id, window.cursor_position());
                return cursor;
            }
            RenderTarget::Image(_) => {
                log::trace!("yabui input detection not supported for image render targets")
            }
            RenderTarget::TextureView(_) => {
                log::trace!("yabui input detection not supported for manual texture render targets")
            }
        }

        None
    }
}

pub(crate) fn update_input_detection_nodes(
    mut commands: Commands,
    input: Res<Input<MouseButton>>,
    mut state: UpdateInputDetectionState,
    mut nodes: Query<(
        Entity,
        &mut LayoutNodeInputDetection,
        &ComputedBoundingBox,
        &LayoutId,
        &LayoutCursors,
    )>,
    custom_cursors: Query<&LayoutCursorPosition>,
) {
    state.cursor_positions.clear();

    let left = input.pressed(MouseButton::Left);
    let right = input.pressed(MouseButton::Right);
    let middle = input.pressed(MouseButton::Middle);

    let just_left = input.just_pressed(MouseButton::Left);
    let just_right = input.just_pressed(MouseButton::Right);
    let just_middle = input.just_pressed(MouseButton::Middle);

    for (entity, mut detection, bounding_box, layout_id, cursors) in nodes.iter_mut() {
        for cursor in cursors.iter() {
            let pos = match cursor {
                Cursor::CameraWindow => {
                    let Some(pos) = state.get_camera_cursors_for_layout(*layout_id) else {
                        continue;
                    };
                    pos
                }
                Cursor::Custom(entity) => {
                    let Ok(cursor) = custom_cursors.get(*entity) else {
                        log::warn!("Custom cursor must have LayoutCursorPosition component");
                        continue;
                    };
                    cursor.position
                }
            };

            let is_in = bounding_box.contains(pos);

            let mut commands = commands.entity(entity);

            let detection = &mut *detection;

            let det_state = detection.state.entry(*cursor).or_default();

            if is_in && !det_state.is_hover {
                det_state.is_hover = true;
                if detection.global_state.inc_hover() {
                    commands.add(CallGlobalEventHandlerCommand(EventKind::Hover));
                }
                commands.add(CallEventHandlerCommand::new(EventKind::Hover, *cursor));
            } else if !is_in && det_state.is_hover {
                det_state.is_hover = false;
                if detection.global_state.dec_hover() {
                    commands.add(CallGlobalEventHandlerCommand(EventKind::Unhover));
                }
                commands.add(CallEventHandlerCommand::new(EventKind::Unhover, *cursor));
            }

            match (det_state.is_left, left) {
                (true, true) | (false, false) => {}
                (false, true) if is_in && just_left => {
                    det_state.is_left = true;
                    if detection.global_state.inc_left() {
                        commands.add(CallGlobalEventHandlerCommand(EventKind::Click));
                    }
                    commands.add(CallEventHandlerCommand::new(EventKind::Click, *cursor));
                }
                (false, true) => {}
                (true, false) => {
                    det_state.is_left = false;
                    if detection.global_state.dec_left() {
                        commands.add(CallGlobalEventHandlerCommand(EventKind::Unclick));
                    }
                    commands.add(CallEventHandlerCommand::new(EventKind::Unclick, *cursor));
                }
            }

            match (det_state.is_right, right) {
                (true, true) | (false, false) => {}
                (false, true) if is_in && just_right => {
                    det_state.is_right = true;
                    if detection.global_state.inc_right() {
                        commands.add(CallGlobalEventHandlerCommand(EventKind::RightClick));
                    }
                    commands.add(CallEventHandlerCommand::new(EventKind::RightClick, *cursor));
                }
                (false, true) => {}
                (true, false) => {
                    det_state.is_right = false;
                    if detection.global_state.dec_right() {
                        commands.add(CallGlobalEventHandlerCommand(EventKind::RightUnclick));
                    }
                    commands.add(CallEventHandlerCommand::new(
                        EventKind::RightUnclick,
                        *cursor,
                    ));
                }
            }

            match (det_state.is_middle, middle) {
                (true, true) | (false, false) => {}
                (false, true) if is_in && just_middle => {
                    det_state.is_middle = true;
                    if detection.global_state.inc_middle() {
                        commands.add(CallGlobalEventHandlerCommand(EventKind::MiddleClick));
                    }
                    commands.add(CallEventHandlerCommand::new(
                        EventKind::MiddleClick,
                        *cursor,
                    ));
                }
                (false, true) => {}
                (true, false) => {
                    det_state.is_middle = false;
                    if detection.global_state.dec_middle() {
                        commands.add(CallGlobalEventHandlerCommand(EventKind::MiddleUnclick));
                    }
                    commands.add(CallEventHandlerCommand::new(
                        EventKind::MiddleUnclick,
                        *cursor,
                    ));
                }
            }
        }
    }
}
