use std::time::Duration;

use druid::im::Vector;
use druid::text::Formatter;
use druid::widget::{self, prelude::*, Button, EnvScope};
use druid::widget::{Flex, Label};
use druid::{
    AppDelegate, AppLauncher, Command, Data, DelegateCtx, ExtEventSink, Handled, Lens, Selector,
    Target, WidgetExt, WindowDesc,
};
use log::info;
use log::{debug, warn};

use crate::controller;
use crate::controller::{ControllerMsg, ControllerSender};
use code_challenge_game_types::gametraits;

pub const UI_UPDATE_COMMAND: Selector<Box<dyn gametraits::GameTrait>> = Selector::new("ui_update");
pub const UI_UPDATE_CONTROLLER_INFO_COMMAND: Selector<controller::ControllerInfo> =
    Selector::new("ui_update_controller_info");

#[derive(Clone, Lens, Data)]
struct AppData {
    #[data(same_fn = "games_eq")]
    game_state: Box<dyn gametraits::GameTrait>,
    what: u32,
    controller_settings: ControllerSettings,
    connected_users: Vector<UiUser>,
    game_mode: GameMode,
}

#[derive(Clone, Data)]
struct UiUser {
    name: String,
    color: druid::Color,
    score: u64,
}

#[derive(Clone, Lens, Data, PartialEq, Eq)]
struct ControllerSettings {
    time_between_turns: std::time::Duration,
    time_after_win: std::time::Duration,
    game_mode: GameMode,
}

impl Default for ControllerSettings {
    fn default() -> Self {
        Self {
            time_between_turns: std::time::Duration::from_millis(100),
            time_after_win: std::time::Duration::from_millis(600),
            game_mode: GameMode::Practice,
        }
    }
}

#[derive(Debug, Clone, Data, PartialEq, Eq)]
enum GameMode {
    Practice,
    Gating,
    Compete,
}

impl From<controller::GameMode> for GameMode {
    fn from(orig: controller::GameMode) -> Self {
        match orig {
            controller::GameMode::Practice => GameMode::Practice,
            controller::GameMode::Gating => GameMode::Gating,
            controller::GameMode::Competition => GameMode::Compete,
        }
    }
}

#[allow(clippy::borrowed_box)]
fn games_eq(left: &Box<dyn gametraits::GameTrait>, right: &Box<dyn gametraits::GameTrait>) -> bool {
    left.eq(&**right) // Hehe...
}
struct GameWidget {}

impl Widget<AppData> for GameWidget {
    fn event(&mut self, _ctx: &mut EventCtx, _event: &Event, _data: &mut AppData, _env: &Env) {}

    fn lifecycle(
        &mut self,
        _ctx: &mut LifeCycleCtx,
        _event: &LifeCycle,
        _data: &AppData,
        _env: &Env,
    ) {
    }

    fn update(&mut self, ctx: &mut UpdateCtx, _old_data: &AppData, _data: &AppData, _env: &Env) {
        debug!("Update in UI");
        ctx.request_paint();
    }

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx,
        bc: &BoxConstraints,
        _data: &AppData,
        _env: &Env,
    ) -> Size {
        bc.max()
    }

    fn paint(&mut self, ctx: &mut PaintCtx, _data: &AppData, _env: &Env) {
        debug!("Druid repainting");
        _data.game_state.paint(ctx);
    }
}

struct Delegate;

impl AppDelegate<AppData> for Delegate {
    fn command(
        &mut self,
        _ctx: &mut DelegateCtx,
        _target: Target,
        cmd: &Command,
        data: &mut AppData,
        _env: &Env,
    ) -> Handled {
        if let Some(new_game_state) = cmd.get(UI_UPDATE_COMMAND) {
            debug!("New game state received");
            data.game_state = new_game_state.clone();
            Handled::Yes
        } else if let Some(info) = cmd.get(UI_UPDATE_CONTROLLER_INFO_COMMAND) {
            debug!("New controller info received");
            data.connected_users = info
                .connected_users
                .iter()
                .map(|gametraits::User { name, color }| UiUser {
                    name: name.clone(),
                    color: *color,
                    score: *info.score.get(name).unwrap_or(&0),
                })
                .collect();
            data.game_mode = info.game_mode.clone().into();
            Handled::Yes
        } else {
            warn!("UI got command, but not handled");
            Handled::No
        }
    }
}

struct DurationFormatter {}
impl Formatter<Duration> for DurationFormatter {
    fn format(&self, dur: &Duration) -> String {
        dur.as_millis().to_string()
    }

    fn validate_partial_input(
        &self,
        input: &str,
        _sel: &druid::text::Selection,
    ) -> druid::text::Validation {
        match input.parse::<u64>() {
            Ok(_) => druid::text::Validation::success(),
            Err(e) => druid::text::Validation::failure(druid::text::ValidationError::new(e)),
        }
    }

    fn value(&self, input: &str) -> Result<Duration, druid::text::ValidationError> {
        input
            .parse::<u64>()
            .map(Duration::from_millis)
            .map_err(druid::text::ValidationError::new)
    }
}

fn make_settings_widget(controller_sender: ControllerSender) -> impl Widget<ControllerSettings> {
    let cs2 = controller_sender.clone();
    let cs3 = controller_sender.clone();
    let cs4 = controller_sender.clone();
    let cs5 = controller_sender;
    Flex::column()
        .with_child(Label::new("Duration after win"))
        .with_child(
            widget::ValueTextBox::new(widget::TextBox::new(), DurationFormatter {})
                .lens(ControllerSettings::time_after_win),
        )
        .with_child(Label::new("Duration between turns"))
        .with_child(
            widget::ValueTextBox::new(widget::TextBox::new(), DurationFormatter {})
                .lens(ControllerSettings::time_between_turns),
        )
        .with_child(Button::new("Apply delays").on_click(
            move |_: &mut EventCtx, settings: &mut ControllerSettings, _: &Env| {
                cs5.send(ControllerMsg::SetTurnDelay(settings.time_between_turns));
                cs5.send(ControllerMsg::SetWinDelay(settings.time_after_win));
            },
        ))
        .with_child(Button::new("Go").on_click(
            move |_: &mut EventCtx, _: &mut ControllerSettings, _: &Env| {
                cs2.send(ControllerMsg::GoToMode(controller::GameMode::Practice))
            },
        ))
        .with_child(Button::new("Gate").on_click(
            move |_: &mut EventCtx, _: &mut ControllerSettings, _: &Env| {
                cs3.send(ControllerMsg::GoToMode(controller::GameMode::Gating))
            },
        ))
        .with_child(Button::new("Reset").on_click(
            move |_: &mut EventCtx, _: &mut ControllerSettings, _: &Env| {
                cs4.send(ControllerMsg::ResetGame);
            },
        ))
}

fn make_widget_connected_users() -> impl Widget<Vector<UiUser>> {
    Flex::column().with_flex_child(
        widget::Scroll::new(widget::List::new(|| {
            EnvScope::new(
                |env, UiUser { color, .. }| env.set(druid::theme::TEXT_COLOR, *color),
                Label::new(|u: &UiUser, _env: &_| format!("* {} - {}", u.name, u.score))
                    .with_text_size(36.0),
            )
        })),
        1.0,
    )
}

fn make_widget_game_mode() -> impl Widget<GameMode> {
    Label::new(|m: &GameMode, _env: &_| format!("{:?}", m.clone()))
}

fn make_widget(controller_sender: ControllerSender) -> impl Widget<AppData> {
    Flex::row()
        .with_child(
            Flex::column()
                .with_flex_child(
                    make_settings_widget(controller_sender).lens(AppData::controller_settings),
                    1.0,
                )
                .with_flex_child(make_widget_game_mode().lens(AppData::game_mode), 1.0)
                .with_flex_child(
                    make_widget_connected_users().lens(AppData::connected_users),
                    1.0,
                ),
        )
        .with_flex_child(GameWidget {}, 1.0)
}

pub fn launch(
    handle_tx: tokio::sync::oneshot::Sender<ExtEventSink>,
    controller_sender: ControllerSender,
) {
    info!("Launching UI");
    let window = WindowDesc::new(make_widget(controller_sender))
        .window_size(Size {
            width: 800.0,
            height: 850.0,
        })
        .resizable(true)
        .title("Game Num");

    let launcher = AppLauncher::with_window(window)
        .delegate(Delegate {})
        .log_to_console();

    info!("Getting handle from launcher");
    let handle = launcher.get_external_handle();
    match handle_tx.send(handle) {
        Ok(_) => (),
        Err(_) => panic!("Unable to send UI handle back to main task"),
    }

    info!("Final launch of UI");
    launcher
        .launch(AppData {
            game_state: crate::games::dumb::make_ptr(vec![]),
            what: 13,
            controller_settings: ControllerSettings::default(),
            connected_users: Vector::new(),
            game_mode: GameMode::Practice,
        })
        .expect("launch failed");
}
