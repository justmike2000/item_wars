//! A small player game done after watching
//! <https://www.youtube.com/watch?v=HCwMb0KslX8>
//! to showcase ggez and how it relates/differs from piston.
//!
//! Note that this example is meant to highlight the general
//! structure of a ggez game. Some of the details may need to
//! be changed to scale the game. For example, if we needed to
//! draw hundreds or thousands of shapes, a SpriteBatch is going
//! to offer far better performance than the direct draw calls
//! that this example uses.
//!
//! Author: @termhn
//! Original repo: https://github.com/termhn/ggez_player

// First we need to actually `use` the pieces of ggez that we are going
// to need frequently.
use ggez::event::{KeyCode, KeyMods};
use ggez::{event, graphics, Context, GameResult};
use graphics::Rect;

// We'll bring in some things from `std` to help us in the future.
use std::time::{Duration, Instant};

// And finally bring the `Rng` trait into scope so that we can generate
// some random numbers later.
use rand::Rng;

// The first thing we want to do is set up some constants that will help us out later.

const SCREEN_SIZE: (f32, f32) = (640.0, 480.0);
const GRID_CELL_SIZE: (f32) = (32.0);

const PLAYER_MAX_HP: (i64) = 100;
const PLAYER_MAX_MP: (i64)= 30;
const PLAYER_MAX_STR: (i64) = 10;
const PLAYER_MOVE_SPEED: (f32) = 10.0;

// Here we're defining how many quickly we want our game to update. This will be
// important later so that we don't have our player fly across the screen because
// it's moving a full tile every frame.
const UPDATES_PER_SECOND: f32 = 30.0;
// And we get the milliseconds of delay that this update rate corresponds to.
const MILLIS_PER_UPDATE: u64 = (1.0 / UPDATES_PER_SECOND * 1000.0) as u64;

#[derive(PartialEq, PartialOrd, Clone, Copy)]
struct Position {
    x: f32,
    y: f32,
}

impl From<Position> for Rect {
    fn from(pos: Position) -> Self {
        Rect { x: pos.x, y: pos.y, w: GRID_CELL_SIZE, h: GRID_CELL_SIZE }
    }
}

/// This is a trait that provides a modulus function that works for negative values
/// rather than just the standard remainder op (%) which does not. We'll use this
/// to get our player to wrap from one side of the game board around to the other
/// when it goes off the top, bottom, left, or right side of the screen.
trait ModuloSigned {
    fn modulo(&self, n: Self) -> Self;
}

/// Here we implement our `ModuloSigned` trait for any type T which implements
/// `Add` (the `+` operator) with an output type T and Rem (the `%` operator)
/// that also has an output type of T, and that can be cloned. These are the bounds
/// that we need in order to implement a modulus function that works for negative numbers
/// as well.
impl<T> ModuloSigned for T
where
    T: std::ops::Add<Output = T> + std::ops::Rem<Output = T> + Clone,
{
    fn modulo(&self, n: T) -> T {
        // Because of our trait bounds, we can now apply these operators.
        (self.clone() % n.clone() + n.clone()) % n.clone()
    }
}

#[derive(Default)]
struct Direction {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
}

/// This is again an abstraction over a `GridPosition` that represents
/// a piece of food the player can eat. It can draw itself.
struct Food {
    pos: Position,
}

impl Food {
    pub fn new(pos: Position) -> Self {
        Food { pos }
    }

    /// Here is the first time we see what drawing looks like with ggez.
    /// We have a function that takes in a `&mut ggez::Context` which we use
    /// with the helpers in `ggez::graphics` to do drawing. We also return a
    /// `ggez::GameResult` so that we can use the `?` operator to bubble up
    /// failure of drawing.
    ///
    /// Note: this method of drawing does not scale. If you need to render
    /// a large number of shapes, use a SpriteBatch. This approach is fine for
    /// this example since there are a fairly limited number of calls.
    fn draw(&self, ctx: &mut Context) -> GameResult<()> {
        // First we set the color to draw with, in this case all food will be
        // colored blue.
        let color = [0.0, 0.0, 1.0, 1.0].into();
        // Then we draw a rectangle with the Fill draw mode, and we convert the
        // Food's position into a `ggez::Rect` using `.into()` which we can do
        // since we implemented `From<GridPosition>` for `Rect` earlier.
        let rectangle =
            graphics::Mesh::new_rectangle(ctx, graphics::DrawMode::fill(), self.pos.into(), color)?;
        graphics::draw(ctx, &rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))
    }
}

/// Here we define an enum of the possible things that the player could have "eaten"
/// during an update of the game. It could have either eaten a piece of `Food`
#[derive(Clone, Copy, Debug)]
enum Ate {
    Food,
}

/// Now we make a struct that contains all the information needed to describe the
/// state of the Player itself.
struct Player {
    /// First we have the body of the player, which is a single `Segment`.
    body: Position,
    /// Then we have the current direction the player is moving. This is
    /// the direction it will move when `update` is called on it.
    dir: Direction,
    /// Now we have a property that represents the result of the last update
    /// that was performed. The player could have eaten nothing (None), Food (Some(Ate::Food)),
    /// or Itself (Some(Ate::Itself))
    ate: Option<Ate>,
    /// Store the direction that will be used in the `update` after the next `update`
    /// This is needed so a user can press two directions (eg. left then up)
    /// before one `update` has happened. It sort of queues up key press input
    next_dir: Option<Direction>,
    hp: i64,
    mp: i64,
    str: i64,
    moving: bool,
}

impl Player {
    pub fn new(pos: Position) -> Self {
        // Our player will initially have a body and one body segment,
        // and will be moving to the right.
        Player {
            body: Position { x: 100.0, y: 100.0 },
            dir: Direction::default(),
            ate: None,
            next_dir: None,
            moving: false,
            hp: PLAYER_MAX_HP,
            mp: PLAYER_MAX_MP,
            str: PLAYER_MAX_STR
        }
    }

    /// A helper function that determines whether
    /// the player eats a given piece of Food based
    /// on its current position
    fn eats(&self, food: &Food) -> bool {
        if self.body == food.pos {
            true
        } else {
            false
        }
    }

    fn move_direction(&mut self) {
            if self.dir.up {
                self.body.y -= PLAYER_MOVE_SPEED;
            }
            if self.dir.down {
                self.body.y += PLAYER_MOVE_SPEED;
            }
            if self.dir.left {
                self.body.x -= PLAYER_MOVE_SPEED;
            }
            if self.dir.right {
                self.body.x += PLAYER_MOVE_SPEED;
            }
    }

    /// The main update function for our player which gets called every time
    /// we want to update the game state.
    fn update(&mut self, food: &Food) {
        if self.moving {
            self.move_direction()
        }
        if self.eats(food) {
            self.ate = Some(Ate::Food);
        } else {
            self.ate = None
        }
    }

    /// Here we have the Player draw itself. This is very similar to how we saw the Food
    /// draw itself earlier.
    ///
    /// Again, note that this approach to drawing is fine for the limited scope of this
    /// example, but larger scale games will likely need a more optimized render path
    /// using SpriteBatch or something similar that batches draw calls.
    fn draw(&self, ctx: &mut Context) -> GameResult<()> {
        // And then we do the same for the head, instead making it fully red to distinguish it.
        let rectangle = graphics::Mesh::new_rectangle(
            ctx,
            graphics::DrawMode::fill(),
            self.body.into(),
            [1.0, 0.5, 0.0, 1.0].into(),
        )?;
        graphics::draw(ctx, &rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))?;
        Ok(())
    }
}

struct Hud {
    player_name: String,
    player_hp: String,
    player_mp: String,
}

impl Hud {

    fn new() -> Hud {
        Hud {
            player_name: "".to_string(),
            player_hp: "".to_string(),
            player_mp: "".to_string(),
        }
    }

    fn draw(&self, ctx: &mut Context) -> GameResult<()> {
        let color = [0.0, 0.0, 0.0, 1.0].into();
        let top_back = graphics::Rect {
                x: 0.0,
                y: 0.0,
                w: 1000.0,
                h: GRID_CELL_SIZE,
        };
        let bottom_back = graphics::Rect {
                x: 0.0,
                y: 608.0,
                w: 1000.0,
                h: GRID_CELL_SIZE,
        };
        let top_rectangle =
            graphics::Mesh::new_rectangle(ctx, graphics::DrawMode::fill(), top_back, color)?;
        graphics::draw(ctx, &top_rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))?;
        let bottom_rectangle =
            graphics::Mesh::new_rectangle(ctx, graphics::DrawMode::fill(), bottom_back, color)?;
        graphics::draw(ctx, &bottom_rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))?;
        let mut text = graphics::Text::new(graphics::TextFragment {
                text: format!("Player: {}", self.player_name),
                color: Some(graphics::Color::new(1.0, 1.0, 1.0, 1.0)),
                // `Font` is a handle to a loaded TTF, stored inside the `Context`.
                // `Font::default()` always exists and maps to DejaVuSerif.
                font: Some(graphics::Font::default()),
                scale: Some(graphics::PxScale { x: 30.0, y: 30.0 }),
                ..Default::default()
            });
        graphics::queue_text(ctx, &text, ggez::mint::Point2 { x: 0.0, y: 0.0 }, None);
        graphics::draw_queued_text(
                ctx,
                graphics::DrawParam::new()
                    .dest(ggez::mint::Point2 { x: 0.0, y: 0.0}),
                    //.rotation(-0.5),
                None,
                graphics::FilterMode::Linear,
            )?;
        Ok(())
    }
}

/// Now we have the heart of our game, the GameState. This struct
/// will implement ggez's `EventHandler` trait and will therefore drive
/// everything else that happens in our game.
struct GameState {
    /// First we need a Player
    player: Player,
    /// A piece of food
    food: Food,
    /// Whether the game is over or not
    gameover: bool,
    /// And we track the last time we updated so that we can limit
    /// our update rate.
    last_update: Instant,
    hud: Hud,
    pressed_keys: Vec<KeyCode>
}

impl GameState {
    /// Our new function will set up the initial state of our game.
    pub fn new() -> Self {
        let mut rng = rand::thread_rng();
        let player_pos = Position { x: 0.0, y: 0.0 };
        let food_pos = Position { x: rng.gen_range(0, SCREEN_SIZE.0 as i16) as f32,
                                          y: rng.gen_range(0, SCREEN_SIZE.1 as i16) as f32 };
        let player = Player::new(player_pos);

        GameState {
            player: player,
            food: Food::new(food_pos),
            hud: Hud::new(),
            gameover: false,
            last_update: Instant::now(),
            pressed_keys: vec![],
        }
    }
}

/// Now we implement EventHandler for GameState. This provides an interface
/// that ggez will call automatically when different events happen.
impl event::EventHandler for GameState {
    /// Update will happen on every frame before it is drawn. This is where we update
    /// our game state to react to whatever is happening in the game world.
    fn update(&mut self, _ctx: &mut Context) -> GameResult {
        // First we check to see if enough time has elapsed since our last update based on
        // the update rate we defined at the top.
        if Instant::now() - self.last_update >= Duration::from_millis(MILLIS_PER_UPDATE) {
            // Then we check to see if the game is over. If not, we'll update. If so, we'll just do nothing.
            if !self.gameover {
                // Here we do the actual updating of our game world. First we tell the player to update itself,
                // passing in a reference to our piece of food.
                self.player.update(&self.food);
                // Next we check if the player ate anything as it updated.
                if let Some(ate) = self.player.ate {
                    // If it did, we want to know what it ate.
                    match ate {
                        // If it ate a piece of food, we randomly select a new position for our piece of food
                        // and move it to this new position.
                        Ate::Food => {
                            let mut rng = rand::thread_rng();
                            self.food.pos = Position { x: rng.gen_range(0, SCREEN_SIZE.0 as i16) as f32,
                                                       y: rng.gen_range(0, SCREEN_SIZE.1 as i16) as f32 }
                        }
                    }
                }
            }
            // If we updated, we set our last_update to be now
            self.last_update = Instant::now();
        }
        // Finally we return `Ok` to indicate we didn't run into any errors
        Ok(())
    }

    /// draw is where we should actually render the game's current state.
    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        // First we clear the screen to a nice (well, maybe pretty glaring ;)) green
        graphics::clear(ctx, [0.0, 0.5, 0.0, 1.0].into());
        // Then we tell the player and the food to draw themselves
        self.player.draw(ctx)?;
        self.food.draw(ctx)?;
        self.hud.draw(ctx)?;
        // Finally we call graphics::present to cycle the gpu's framebuffer and display
        // the new frame we just drew.
        graphics::present(ctx)?;
        // We yield the current thread until the next update
        ggez::timer::yield_now();
        // And return success.
        Ok(())
    }

    fn key_up_event(
        &mut self,
        _ctx: &mut Context,
        keycode: KeyCode,
        _keymod: KeyMods,
    ) {
        match keycode {
            KeyCode::Left => self.player.dir.left = false,
            KeyCode::Right => self.player.dir.right = false,
            KeyCode::Up => self.player.dir.up = false,
            KeyCode::Down => self.player.dir.down = false,
            _ => ()
        };
        self.player.moving = false;
    }

    /// key_down_event gets fired when a key gets pressed.
    fn key_down_event(
        &mut self,
        _ctx: &mut Context,
        keycode: KeyCode,
        _keymod: KeyMods,
        _repeat: bool,
    ) {
        match keycode {
            KeyCode::Left => self.player.dir.left = true,
            KeyCode::Right => self.player.dir.right = true,
            KeyCode::Up => self.player.dir.up = true,
            KeyCode::Down => self.player.dir.down = true,
            _ => ()
        };
        self.player.moving = true;
    }
}

fn main() -> GameResult {
    // Here we use a ContextBuilder to setup metadata about our game. First the title and author
    let (ctx, events_loop) = ggez::ContextBuilder::new("player", "Gray Olson")
        // Next we set up the window. This title will be displayed in the title bar of the window.
        .window_setup(ggez::conf::WindowSetup::default().title("Player!"))
        // Now we get to set the size of the window, which we use our SCREEN_SIZE constant from earlier to help with
        .window_mode(ggez::conf::WindowMode::default().dimensions(SCREEN_SIZE.0, SCREEN_SIZE.1))
        // And finally we attempt to build the context and create the window. If it fails, we panic with the message
        // "Failed to build ggez context"
        .build()?;

    // Next we create a new instance of our GameState struct, which implements EventHandler
    let state = GameState::new();
    // And finally we actually run our game, passing in our context and state.
    event::run(ctx, events_loop, state)
}
