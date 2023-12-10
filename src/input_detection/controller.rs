use crate::views::NodeEntityMut;
use bevy::{
    prelude::*,
    utils::{HashMap, HashSet},
};
use smallvec::SmallVec;

#[derive(Component, Default)]
pub struct UiInputCommands {
    commands: HashMap<UiInput, Vec<Box<dyn InputDetectionCommand>>>,
}

impl UiInputCommands {
    pub fn on_press(&mut self, input: UiInput, command: impl InputDetectionCommand) -> &mut Self {
        self.commands
            .entry(input)
            .or_default()
            .push(Box::new(command));
        self
    }
}

enum FocusableNodeInternal {
    Global {
        focused: bool,
        was_focus_changed: bool,
        focus: Vec<Box<dyn FocusDetectionCommand>>,
        unfocus: Vec<Box<dyn FocusDetectionCommand>>,
    },
    Local {
        sources: HashSet<FocusSource>,
        added: Vec<FocusSource>,
        removed: Vec<FocusSource>,
        focus: Vec<Box<dyn FocusDetectionCommand>>,
        unfocus: Vec<Box<dyn FocusDetectionCommand>>,
    },
}

#[derive(Component)]
pub struct FocusableNode(FocusableNodeInternal);

impl FocusableNode {
    pub fn global() -> Self {
        Self(FocusableNodeInternal::Global {
            focused: false,
            was_focus_changed: false,
            focus: vec![],
            unfocus: vec![],
        })
    }

    pub fn local() -> Self {
        Self(FocusableNodeInternal::Local {
            sources: HashSet::new(),
            added: vec![],
            removed: vec![],
            focus: vec![],
            unfocus: vec![],
        })
    }

    pub fn is_focus(&self) -> bool {
        match &self.0 {
            FocusableNodeInternal::Global { focused, .. } => *focused,
            FocusableNodeInternal::Local { sources, .. } => !sources.is_empty(),
        }
    }

    pub fn is_focused_by(&self, source: FocusSource) -> bool {
        match &self.0 {
            FocusableNodeInternal::Global { focused, .. } => *focused,
            FocusableNodeInternal::Local { sources, .. } => sources.contains(&source),
        }
    }

    pub fn focus(&mut self) {
        self.focus_with(FocusSource::External);
    }

    pub fn focus_with(&mut self, source: FocusSource) {
        match &mut self.0 {
            FocusableNodeInternal::Global {
                focused,
                was_focus_changed,
                ..
            } => {
                *was_focus_changed = !*focused;
                *focused = true;
            }
            FocusableNodeInternal::Local { sources, added, .. } => {
                if sources.insert(source) {
                    added.push(source);
                }
            }
        }
    }

    pub fn unfocus(&mut self, source: FocusSource) {
        match &mut self.0 {
            FocusableNodeInternal::Global {
                focused,
                was_focus_changed,
                ..
            } => {
                *was_focus_changed = *focused;
                *focused = false
            }
            FocusableNodeInternal::Local {
                sources, removed, ..
            } => {
                if sources.remove(&source) {
                    removed.push(source);
                }
            }
        }
    }

    pub fn unfocus_all(&mut self) {
        match &mut self.0 {
            FocusableNodeInternal::Global { focused, .. } => *focused = false,
            FocusableNodeInternal::Local {
                sources, removed, ..
            } => {
                removed.extend(sources.iter().copied());
                sources.clear();
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum UiInput {
    /// This should correspond to the default/primary face button on controllers (i.e. the "A" button)
    Decide,

    /// This should correspond to the secondary face button on controllers (i.e. the "B" button)
    Cancel,

    /// This should correspond to either of the the two other face buttons on controllers (i.e.
    /// "X/Y")
    Other1,

    /// This should correspond to either of the two other face buttons on controllers (i.e. "X/Y")
    Other2,

    /// This should correspond to either the front or back shoulder button (on the left) of a
    /// controller
    ///
    /// It doesn't matter if this is the front or the back, but it should be consistent with
    /// whatever [`Self::RotateR1`] is mapped to for consistency in the user experience
    RotateL1,

    /// This should correspond to either the front or back shoulder button (on the left) of a
    /// controller
    ///
    /// It doesn't matter if this is the front or the back, but it should be consistent with
    /// whatever [`Self::RotateR2`] is mapped to for consistency in the user experience
    RotateL2,

    /// This should correspond to either the front or back shoulder button (on the right) of a
    /// controller
    ///
    /// It doesn't matter if this is the front or the back, but it should be consistent with
    /// whatever [`Self::RotateL1`] is mapped to for consistency in the user experience
    RotateR1,

    /// This should correspond to either the front or back shoulder button (on the right) of a
    /// controller
    ///
    /// It doesn't matter if this is the front or the back, but it should be consistent with
    /// whatever [`Self::RotateL2`] is mapped to for consistency in the user experience
    RotateR2,

    /// This should correspond to the right-middle button on a controller
    Start,

    /// This should correspond to the left-middle button on a controller
    Select,
}

#[derive(Resource)]
pub struct UiInputMap {
    keyboard: HashMap<KeyCode, UiInput>,
    controllers: HashMap<Gamepad, HashMap<GamepadButtonType, UiInput>>,
}

impl Default for UiInputMap {
    fn default() -> Self {
        Self {
            keyboard: Self::default_keyboard(),
            controllers: Default::default(),
        }
    }
}

macro_rules! hm {
    ($($key:expr => $value:expr),*) => {
        {
            let mut __map = HashMap::new();
            $(
                __map.insert($key, $value);
            )*
            __map
        }
    }
}

impl UiInputMap {
    pub fn default_keyboard() -> HashMap<KeyCode, UiInput> {
        hm! {
            KeyCode::Q => UiInput::RotateL1,
            KeyCode::E => UiInput::RotateR1,
            KeyCode::J => UiInput::RotateL2,
            KeyCode::K => UiInput::RotateR2,
            KeyCode::Z => UiInput::Decide,
            KeyCode::X => UiInput::Cancel,
            KeyCode::C => UiInput::Other1,
            KeyCode::F => UiInput::Other2,
            KeyCode::Return => UiInput::Start,
            KeyCode::Escape => UiInput::Select
        }
    }

    pub fn default_controller() -> HashMap<GamepadButtonType, UiInput> {
        use GamepadButtonType as G;
        use UiInput as U;
        hm! {
            G::South => U::Decide,
            G::East => U::Cancel,
            G::North => U::Other1,
            G::West => U::Other2,
            G::LeftTrigger => U::RotateL1,
            G::LeftTrigger2 => U::RotateL2,
            G::RightTrigger => U::RotateR1,
            G::RightTrigger2 => U::RotateR2,
            G::Start => U::Start,
            G::Select => U::Select
        }
    }

    pub fn reset_keyboard(&mut self) {
        self.keyboard = Self::default_keyboard();
    }

    pub fn reset_controller(&mut self, gamepad: Gamepad) {
        *self.controllers.entry(gamepad).or_default() = Self::default_controller();
    }

    pub fn submit_keyboard(&mut self, map: HashMap<KeyCode, UiInput>) {
        self.keyboard = map;
    }

    pub fn submit_controller(
        &mut self,
        gamepad: Gamepad,
        map: HashMap<GamepadButtonType, UiInput>,
    ) {
        *self.controllers.entry(gamepad).or_default() = map;
    }
}

/// The source of a controller UI input
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum InputSource {
    /// The input was generated from a keyboard
    Keyboard,

    /// The input was generated from a controller
    Controller(Gamepad),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum FocusSource {
    External,
    Keyboard,
    Controller(Gamepad),
}

/// Trait for something to run when an input is detected
pub trait InputDetectionCommand: Send + Sync + 'static {
    fn apply(&mut self, source: InputSource, node: NodeEntityMut);
}

pub trait FocusDetectionCommand: Send + Sync + 'static {
    fn apply(&mut self, source: FocusSource, node: NodeEntityMut);
}

impl<F: FnMut(InputSource, NodeEntityMut) + Send + Sync + 'static> InputDetectionCommand for F {
    fn apply(&mut self, source: InputSource, node: NodeEntityMut) {
        (self)(source, node);
    }
}

impl<F: FnMut(FocusSource, NodeEntityMut) + Send + Sync + 'static> FocusDetectionCommand for F {
    fn apply(&mut self, source: FocusSource, node: NodeEntityMut) {
        (self)(source, node)
    }
}

pub struct SendEvent<E: Event + Clone>(E);

impl<E: Event + Clone> InputDetectionCommand for SendEvent<E> {
    fn apply(&mut self, _source: InputSource, mut node: NodeEntityMut) {
        node.world_mut().send_event(self.0.clone());
    }
}

impl<E: Event + Clone> FocusDetectionCommand for SendEvent<E> {
    fn apply(&mut self, _source: FocusSource, mut node: NodeEntityMut) {
        node.world_mut().send_event(self.0.clone());
    }
}

pub(crate) fn update_focus_nodes(world: &mut World) {
    let mut entities_to_focus: SmallVec<[(Entity, FocusSource); 4]> = SmallVec::new();
    let mut entities_to_unfocus: SmallVec<[(Entity, FocusSource); 4]> = SmallVec::new();

    world
        .query_filtered::<(Entity, &mut FocusableNode), Changed<FocusableNode>>()
        .iter_mut(world)
        .for_each(|(entity, mut node)| {
            let node = node.bypass_change_detection();

            match &mut node.0 {
                FocusableNodeInternal::Global {
                    was_focus_changed,
                    focused,
                    ..
                } => {
                    if std::mem::take(was_focus_changed) {
                        if *focused {
                            entities_to_focus.push((entity, FocusSource::External));
                        } else {
                            entities_to_unfocus.push((entity, FocusSource::External));
                        }
                    }
                }
                FocusableNodeInternal::Local { added, removed, .. } => {
                    for source in added.iter().copied() {
                        entities_to_focus.push((entity, source));
                    }

                    for source in removed.iter().copied() {
                        entities_to_unfocus.push((entity, source));
                    }

                    added.clear();
                    removed.clear();
                }
            }
        });

    for (entity, source) in entities_to_focus {
        let mut node = NodeEntityMut::new(world, entity);
        let mut focusable = std::mem::replace(
            node.get_mut::<FocusableNode>()
                .unwrap()
                .bypass_change_detection(),
            FocusableNode::global(),
        );

        match &mut focusable.0 {
            FocusableNodeInternal::Global { focus, .. }
            | FocusableNodeInternal::Local { focus, .. } => focus
                .iter_mut()
                .for_each(|cb| cb.apply(source, node.reborrow())),
        }

        *node
            .get_mut::<FocusableNode>()
            .unwrap()
            .bypass_change_detection() = focusable;
    }

    for (entity, source) in entities_to_unfocus {
        let mut node = NodeEntityMut::new(world, entity);
        let mut focusable = std::mem::replace(
            node.get_mut::<FocusableNode>()
                .unwrap()
                .bypass_change_detection(),
            FocusableNode::global(),
        );

        match &mut focusable.0 {
            FocusableNodeInternal::Global { unfocus, .. }
            | FocusableNodeInternal::Local { unfocus, .. } => unfocus
                .iter_mut()
                .for_each(|cb| cb.apply(source, node.reborrow())),
        }

        *node
            .get_mut::<FocusableNode>()
            .unwrap()
            .bypass_change_detection() = focusable;
    }
}

pub(crate) fn update_input_detection(world: &mut World) {
    let mut inputs: SmallVec<[(UiInput, InputSource); 4]> = SmallVec::new();

    world.resource_scope::<UiInputMap, _>(|world, mut mappings| {
        let gamepads = world.resource::<Gamepads>();
        let gp_buttons = world.resource::<Input<GamepadButton>>();
        let kb_buttons = world.resource::<Input<KeyCode>>();

        for (code, input) in mappings.keyboard.iter() {
            if kb_buttons.just_pressed(*code) {
                inputs.push((*input, InputSource::Keyboard));
            }
        }

        for gamepad in gamepads.iter() {
            let mappings = mappings
                .controllers
                .entry(gamepad)
                .or_insert_with(|| UiInputMap::default_controller());
            for (button, input) in mappings.iter() {
                if gp_buttons.just_pressed(GamepadButton::new(gamepad, *button)) {
                    inputs.push((*input, InputSource::Controller(gamepad)));
                }
            }
        }
    });

    let mut entities: SmallVec<[(UiInput, Entity, InputSource); 4]> = SmallVec::new();

    world
        .query::<(Entity, &UiInputCommands, Option<&FocusableNode>)>()
        .iter(world)
        .for_each(|(entity, commands, focus)| {
            for (button, source) in inputs.iter() {
                if let Some(focus) = focus {
                    let focus_source = match source {
                        InputSource::Keyboard => FocusSource::Keyboard,
                        InputSource::Controller(gamepad) => FocusSource::Controller(*gamepad),
                    };

                    if !focus.is_focused_by(FocusSource::External)
                        && !focus.is_focused_by(focus_source)
                    {
                        continue;
                    }
                }

                if commands.commands.contains_key(button) {
                    entities.push((*button, entity, *source));
                }
            }
        });

    for (button, entity, source) in entities {
        let mut node = NodeEntityMut::new(world, entity);
        let mut callbacks = std::mem::take(
            node.get_mut::<UiInputCommands>()
                .unwrap()
                .bypass_change_detection(),
        );
        for command in callbacks.commands.get_mut(&button).unwrap().iter_mut() {
            command.apply(source, node.reborrow());
        }
    }
}
