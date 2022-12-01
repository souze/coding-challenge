use itertools::Itertools;
use std::{any::Any, iter::repeat};

use crate::gametraits::{self, GameTrait, PlayerMoveResult, User};
use druid::{
    kurbo::Line,
    piet::{Text, TextLayoutBuilder},
    Color, FontFamily, Point, Rect, RenderContext,
};
use log::debug;
use serde::{Deserialize, Serialize, Serializer};

#[derive(Debug, Deserialize)]
pub struct PlayerMove {
    x: usize,
    y: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    board: Board,
    winner: Option<(User, FirstAndLast)>,
}

#[derive(Clone, Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
struct Board {
    cells: Vec<Cell>,
    width: usize,
    height: usize,
}

#[derive(Clone, Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum Cell {
    Empty,
    #[serde(serialize_with = "ser_occupied")]
    Occupied(User),
}

// fn<S>(&T, S) -> Result<S::Ok, S::Error> where S: Serializer
fn ser_occupied<S>(user: &User, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_str(&user.name)
}

type FirstAndLast = ((i32, i32), (i32, i32));

impl Board {
    fn try_place(&mut self, user: &User, x: usize, y: usize) -> PlaceResult {
        match self.at_mut(x as i32, y as i32) {
            None => PlaceResult::InvalidMove,
            Some(Cell::Occupied(_)) => PlaceResult::InvalidMove,
            Some(cell @ Cell::Empty) => {
                *cell = Cell::Occupied(user.clone());
                self.check_for_win_around(x, y)
            }
        }
    }

    fn check_for_win_around(&self, x: usize, y: usize) -> PlaceResult {
        let x = x as i32;
        let y = y as i32;

        let winning_coords = self
            .range_contains_win(repeat(x).take(9), y - 4..y + 5)
            .or_else(|| {
                self.range_contains_win(x - 4..x + 5, y - 4..y + 5)
                    .or_else(|| {
                        self.range_contains_win(x - 4..x + 5, repeat(y).take(9))
                            .or_else(|| self.range_contains_win(x - 4..x + 5, (y - 4..y + 5).rev()))
                    })
            });

        match winning_coords {
            Some(coords) => PlaceResult::Win(coords),
            None => PlaceResult::Ok,
        }
    }

    fn at(&self, x: i32, y: i32) -> Option<&Cell> {
        if x < 0 || y < 0 || x >= (self.width as i32) || y >= (self.height as i32) {
            None
        } else {
            Some(&self.cells[(y as usize) * self.width + (x as usize)])
        }
    }

    fn at_mut(&mut self, x: i32, y: i32) -> Option<&mut Cell> {
        if x < 0 || y < 0 || x >= (self.width as i32) || y >= (self.height as i32) {
            None
        } else {
            Some(&mut self.cells[(y as usize) * self.width + (x as usize)])
        }
    }

    fn range_contains_win<T1, T2>(&self, x_range: T1, y_range: T2) -> Option<FirstAndLast>
    where
        T1: Iterator<Item = i32>,
        T2: Iterator<Item = i32>,
    {
        // Now for the tricky part
        std::iter::zip(x_range, y_range)
            .map(|(x, y)| (self.at(x, y), x, y))
            .group_by(|(a, _, _)| *a)
            .into_iter()
            .map(|(_, b)| b.map(|(_, x, y)| (x, y))) // Discard the key and cell
            .into_iter()
            .map(|a| a.collect::<Vec<(i32, i32)>>())
            .max_by(|a, b| a.len().cmp(&b.len()))
            .and_then(|a| {
                if a.len() >= 5 {
                    Some((*a.first().unwrap(), *a.last().unwrap()))
                } else {
                    None
                }
            })
    }

    fn is_full(&self) -> bool {
        !self.cells.iter().any(|c| matches!(c, Cell::Empty))
    }
}

enum PlaceResult {
    Ok,
    Win(FirstAndLast),
    InvalidMove,
}

impl Game {
    fn new(w: usize, h: usize) -> Self {
        Self {
            board: Board {
                width: w,
                height: h,
                cells: repeat(Cell::Empty).take(w * h).collect::<Vec<Cell>>(),
            },
            winner: None,
        }
    }
}

impl gametraits::GameTrait for Game {
    fn player_moves(
        &mut self,
        user: &User,
        player_move: gametraits::PlayerMove,
    ) -> PlayerMoveResult {
        debug!("{user:?} made a move {player_move:?}");
        match gametraits::to_player_move::<PlayerMove>(&player_move) {
            Some(mov) => make_move(self, user, mov),
            None => PlayerMoveResult::InvalidFormat,
        }
    }

    fn get_player_state(&self, _user: &User) -> gametraits::PlayerGameState {
        gametraits::to_game_state(&self.board)
    }

    fn paint(&self, ctx: &mut druid::PaintCtx) {
        let cell_width = (ctx.size().width / self.board.width as f64)
            .min(ctx.size().height / self.board.height as f64);

        const CELL_SPACING: f64 = 2_f64;
        let cell_size: (f64, f64) = (cell_width - CELL_SPACING, cell_width - CELL_SPACING);
        let c_empty = Color::rgb8(0xFF, 0xFF, 0xFF);
        for x in 0..self.board.width {
            for y in 0..self.board.height {
                let rect = Rect::from_origin_size(
                    Point {
                        x: x as f64 * cell_width,
                        y: y as f64 * cell_width,
                    },
                    cell_size,
                );
                let col = match self.board.at(x as i32, y as i32) {
                    Some(Cell::Occupied(User { color, .. })) => color,
                    _ => &c_empty,
                };

                ctx.fill(rect, col);
            }
        }

        if let Some((winner, ((x1, y1), (x2, y2)))) = &self.winner {
            let start_point = Point {
                x: *x1 as f64 * cell_width + cell_width / 2_f64,
                y: *y1 as f64 * cell_width + cell_width / 2_f64,
            };
            let end_point = Point {
                x: *x2 as f64 * cell_width + cell_width / 2_f64,
                y: *y2 as f64 * cell_width + cell_width / 2_f64,
            };

            ctx.stroke(Line::new(start_point, end_point), &Color::PURPLE, 5.0);

            // This is the builder-style way of drawing text.
            let win_text = format!("Winner! {:?}", winner);
            let text = ctx.text();
            let layout = text
                .new_text_layout(win_text)
                .font(FontFamily::SERIF, 24.0)
                .text_color(Color::rgb8(0, 0, 0))
                .build()
                .unwrap();
            ctx.draw_text(&layout, (100.0, 25.0));
        } else if self.board.is_full() {
            let draw_text = "Draw!".to_string();
            let text = ctx.text();
            let layout = text
                .new_text_layout(draw_text)
                .font(FontFamily::SERIF, 24.0)
                .text_color(Color::rgb8(0, 0, 0))
                .build()
                .unwrap();
            ctx.draw_text(&layout, (100.0, 25.0));
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn eq(&self, other: &dyn GameTrait) -> bool {
        self == other.as_any().downcast_ref::<Game>().unwrap()
    }

    fn reset(&mut self) {
        *self = Game::new(self.board.width, self.board.height);
    }
}

pub fn make_ptr(w: usize, h: usize) -> Box<dyn GameTrait> {
    Box::new(Game::new(w, h))
}

fn make_move(state: &mut Game, user: &User, p_move: PlayerMove) -> PlayerMoveResult {
    match state.board.try_place(user, p_move.x, p_move.y) {
        PlaceResult::InvalidMove => PlayerMoveResult::InvalidMove,
        PlaceResult::Ok => {
            if state.board.is_full() {
                PlayerMoveResult::Draw
            } else {
                PlayerMoveResult::Ok
            }
        }
        PlaceResult::Win(coords) => {
            state.winner = Some((user.clone(), coords));
            PlayerMoveResult::Win
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::gametraits::PlayerMoveResult;
    macro_rules! test_init {
        ($game:ident, $p1:ident, $p2:ident, $p3:ident, $mov_ok:ident) => {
            let mut $game = Game::new(10, 10);
            let $p1 = User {
                name: "player1".to_string(),
                color: Color::rgb8(0, 0, 0),
            };
            let $p2 = User {
                name: "player2".to_string(),
                color: Color::rgb8(100, 100, 100),
            };
            let $p3 = User {
                name: "player3".to_string(),
                color: Color::rgb8(200, 200, 200),
            };
            let mut $mov_ok = |u, x, y| {
                assert_eq!(
                    make_move(&mut $game, u, PlayerMove { x, y }),
                    PlayerMoveResult::Ok
                );
            };
        };
    }

    #[test]
    fn invalid_move_space_occupied() {
        test_init!(game, p1, _p2, _p3, mov_ok);
        mov_ok(&p1, 9, 5);
        assert_eq!(
            make_move(&mut game, &p1, PlayerMove { x: 9, y: 5 }),
            PlayerMoveResult::InvalidMove
        );
    }

    #[test]
    fn normal_game() {
        test_init!(game, p1, p2, _p3, mov_ok);
        mov_ok(&p1, 5, 5);
        mov_ok(&p2, 5, 6);
        mov_ok(&p1, 6, 5);
        mov_ok(&p2, 5, 7);
        mov_ok(&p1, 7, 5);
        mov_ok(&p2, 5, 8);
        mov_ok(&p1, 8, 5);
        mov_ok(&p2, 5, 9);
        assert_eq!(
            make_move(&mut game, &p1, PlayerMove { x: 9, y: 5 }),
            PlayerMoveResult::Win
        );
    }

    #[test]
    fn different_players_dont_streak() {
        test_init!(game, p1, p2, _p3, mov_ok);

        mov_ok(&p1, 0, 0);
        mov_ok(&p1, 1, 0);
        mov_ok(&p1, 2, 0);
        mov_ok(&p1, 3, 0);
        mov_ok(&p2, 4, 0);
        mov_ok(&p1, 5, 0);
        mov_ok(&p1, 6, 0);
        mov_ok(&p1, 7, 0);
        mov_ok(&p1, 8, 0);
        assert_eq!(
            make_move(&mut game, &p1, PlayerMove { x: 9, y: 0 }),
            PlayerMoveResult::Win
        )
    }

    #[test]
    fn win_right() {
        test_init!(game, p1, _p2, _p3, mov_ok);

        mov_ok(&p1, 0, 0);
        mov_ok(&p1, 1, 0);
        mov_ok(&p1, 2, 0);
        mov_ok(&p1, 3, 0);
        assert_eq!(
            make_move(&mut game, &p1, PlayerMove { x: 4, y: 0 }),
            PlayerMoveResult::Win
        );
    }

    #[test]
    fn win_down() {
        test_init!(game, p1, _p2, _p3, mov_ok);

        mov_ok(&p1, 0, 0);
        mov_ok(&p1, 0, 1);
        mov_ok(&p1, 0, 2);
        mov_ok(&p1, 0, 3);
        assert_eq!(
            make_move(&mut game, &p1, PlayerMove { x: 0, y: 4 }),
            PlayerMoveResult::Win
        );
    }

    #[test]
    fn win_down_right() {
        test_init!(game, p1, _p2, _p3, mov_ok);

        mov_ok(&p1, 0, 0);
        mov_ok(&p1, 1, 1);
        mov_ok(&p1, 2, 2);
        mov_ok(&p1, 3, 3);
        assert_eq!(
            make_move(&mut game, &p1, PlayerMove { x: 4, y: 4 }),
            PlayerMoveResult::Win
        );
    }

    #[test]
    fn win_down_left() {
        test_init!(game, p1, _p2, _p3, mov_ok);

        // [_ _ _ _ x]
        // [_ _ _ x _]
        // [_ _ x _ _]
        // [_ x _ _ _]
        // [x _ _ _ _]

        mov_ok(&p1, 4, 1);
        mov_ok(&p1, 3, 2);
        mov_ok(&p1, 2, 3);
        mov_ok(&p1, 1, 4);
        assert_eq!(
            make_move(&mut game, &p1, PlayerMove { x: 0, y: 5 }),
            PlayerMoveResult::Win
        );
    }
}
