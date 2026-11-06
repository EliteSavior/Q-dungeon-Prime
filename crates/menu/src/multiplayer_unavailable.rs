use bevy::prelude::*;
use de_gui::{GuiCommands, LabelCommands, OuterStyle};

use crate::{menu::Menu, MenuState};

pub(crate) struct MultiplayerPlugin;

impl Plugin for MultiplayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(MenuState::Multiplayer), setup);
    }
}

fn setup(mut commands: GuiCommands, menu: Res<Menu>) {
    let text_id = commands
        .spawn_label(
            OuterStyle::default(),
            "Multiplayer is not included in this Android build.",
        )
        .id();
    commands.entity(menu.root_node()).add_child(text_id);
}
