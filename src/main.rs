//! Author: @justmike2000
//! Repo: https://github.com/justmike2000/item_wars/

use ggez::event::{KeyCode, KeyMods};
use ggez::{event, graphics, Context, GameResult, timer};
use graphics::{GlBackendSpec, ImageGeneric, Rect};
use glam::*;

use std::time::{Duration, Instant};
use std::io;
use std::path;
use std::env;
use std::collections::HashMap;
use std::io::prelude::*;
use std::net::TcpListener;
use std::net::TcpStream;
use std::io::{Read, Write};
use std::str::from_utf8;

use clap::{Arg, App};
use rand::Rng;
use uuid::Uuid;

// The first thing we want to do is set up some constants that will help us out later.

const SCREEN_SIZE: (f32, f32) = (640.0, 480.0);
const GRID_CELL_SIZE: f32 = 32.0;

const PLAYER_MAX_HP: i64 = 100;
const PLAYER_MAX_MP: i64 = 30;
const PLAYER_MAX_STR: i64 = 10;
const PLAYER_MOVE_SPEED: f32 = 3.0;
const PLAYER_TOP_ACCEL_SPEED: f32 = 5.0;
const PLAYER_ACCEL_SPEED: f32 = 0.2;
const PLAYER_STARTING_ACCEL: f32 = 0.4;
const PLAYER_JUMP_HEIGHT: f32 = 0.5;
const PLAYER_CELL_HEIGHT: f32 = 42.0;
const PLAYER_CELL_WIDTH: f32 = 32.0;

const MAP_CURRENT_FRICTION: f32 = 5.0;

const PACKET_SIZE: usize = 65_000;

const UPDATES_PER_SECOND: f32 = 30.0;
const MILLIS_PER_UPDATE: u64 = (1.0 / UPDATES_PER_SECOND * 1000.0) as u64;

#[derive(PartialEq, PartialOrd, Clone, Copy, Debug)]
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

#[derive(Default, Debug)]
struct Direction {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
}

/// This is again an abstraction over a `GridPosition` that represents
/// a piece of food the player can eat. It can draw itself.
struct Potion {
    pos: Position,
    texture: ImageGeneric<GlBackendSpec>,
}

impl Potion {
    pub fn new(pos: Position, texture: ImageGeneric<GlBackendSpec>) -> Self {
        Potion {
            pos,
            texture
        }
    }

    fn draw(&self, ctx: &mut Context) -> GameResult<()> {
        //let color = [0.0, 0.0, 1.0, 1.0].into();
        //let rectangle =
        //    graphics::Mesh::new_rectangle(ctx, graphics::DrawMode::fill(), self.pos.into(), color)?;
        //graphics::draw(ctx, &rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))
        let param = graphics::DrawParam::new()
        .src(graphics::Rect {x: 0.0, y: 0.0, w: 0.33, h: 0.33})
        .dest(Vec2::new(self.pos.x, self.pos.y))
        //.offset(Vec2::new(0.15, 0.0))
        .scale(Vec2::new(0.25, 0.25));
        //.rotation((time % cycle) as f32 / cycle as f32 * 6.28)
        //.offset(Vec2::new(150.0, 150.0));
        graphics::draw(ctx, &self.texture, param)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
enum Ate {
    Potion,
}
#[derive(Debug)]
struct Player {
    /// First we have the body of the player, which is a single `Segment`.
    body: Position,
    /// Then we have the current direction the player is moving. This is
    /// the direction it will move when `update` is called on it.
    dir: Direction,
    last_dir: Direction,
    ate: Option<Ate>,
    /// Store the direction that will be used in the `update` after the next `update`
    /// This is needed so a user can press two directions (eg. left then up)
    /// before one `update` has happened. It sort of queues up key press input
    name: String,
    hp: i64,
    mp: i64,
    str: i64,
    current_accel: f32,
    jumping: bool,
    jump_offset: f32,
    jump_direction: bool, // true up false down
    texture: ImageGeneric<GlBackendSpec>,
    animation_frame: f32,
    animation_total_frames: f32,
    last_animation: std::time::Instant,
    animation_duration: std::time::Duration,
}

impl Player {
    pub fn new(name: String, pos: Position, texture: ImageGeneric<GlBackendSpec>) -> Self {
        // Our player will initially have a body and one body segment,
        // and will be moving to the right.
        Player {
            name,
            body: pos,
            dir: Direction::default(),
            last_dir: Direction::default(),
            ate: None,
            current_accel: PLAYER_STARTING_ACCEL,
            hp: PLAYER_MAX_HP,
            mp: PLAYER_MAX_MP,
            str: PLAYER_MAX_STR,
            texture: texture,
            jumping: false,
            jump_offset: 0.0,
            jump_direction: true,
            animation_frame: 0.0,
            animation_total_frames: 4.0,
            last_animation: std::time::Instant::now(),
            animation_duration:  Duration::new(0, 150_000_000),
        }
    }

    fn eats(&self, food: &Potion) -> bool {
        if self.body == food.pos {
            true
        } else {
            false
        }
    }

    fn reset_last_dir(&mut self) {
        self.last_dir.left = false;
        self.last_dir.right = false;
        self.last_dir.up = false;
        self.last_dir.down = false;
    }

    fn move_direction(&mut self) {
        self.reset_last_dir();
        if self.current_accel < PLAYER_TOP_ACCEL_SPEED {
            self.current_accel += PLAYER_ACCEL_SPEED;
        }
        if self.dir.up && self.body.y > PLAYER_CELL_HEIGHT {
            self.body.y -= PLAYER_MOVE_SPEED + self.current_accel;
            self.last_dir.up = true;
        }
        if self.dir.down && self.body.y < SCREEN_SIZE.1 - (PLAYER_CELL_HEIGHT * 2.0) {
            self.body.y += PLAYER_MOVE_SPEED + self.current_accel;
            self.last_dir.down = true;
        }
        if self.dir.left && self.body.x > 0.0 {
            self.body.x -= PLAYER_MOVE_SPEED + self.current_accel;
            self.last_dir.left = true;
        }
        if self.dir.right && self.body.x < SCREEN_SIZE.0 - PLAYER_CELL_WIDTH {
            self.body.x += PLAYER_MOVE_SPEED + self.current_accel;
            self.last_dir.right = true;
        }
    }

    fn move_direction_cooldown(&mut self) {
            if self.last_dir.up && self.body.y > PLAYER_CELL_HEIGHT {
                self.body.y -= PLAYER_MOVE_SPEED + self.current_accel;
            }
            if self.last_dir.down && self.body.y < SCREEN_SIZE.1 - (PLAYER_CELL_HEIGHT * 2.0) {
                self.body.y += PLAYER_MOVE_SPEED + self.current_accel;
            }
            if self.last_dir.left && self.body.x > 0.0 {
                self.body.x -= PLAYER_MOVE_SPEED + self.current_accel;
            }
            if self.last_dir.right && self.body.x < SCREEN_SIZE.0 - PLAYER_CELL_WIDTH {
                self.body.x += PLAYER_MOVE_SPEED + self.current_accel;
            }
            if self.current_accel > 0.0 {
                self.current_accel -= PLAYER_ACCEL_SPEED * MAP_CURRENT_FRICTION;
            }
    }

    fn is_moving(&self) -> bool {
        self.dir.up || self.dir.down || self.dir.left || self.dir.right
    }

    fn update(&mut self, food: &Potion) {
        if self.jumping {
            if self.jump_direction && self.jump_offset < PLAYER_JUMP_HEIGHT {
                self.jump_offset += 0.1;
            } else if self.jump_direction && self.jump_offset == PLAYER_JUMP_HEIGHT {
                self.jump_direction = false;
            } else if !self.jump_direction && self.jump_offset <= PLAYER_JUMP_HEIGHT && self.jump_offset > 0.0 {
                self.jump_offset -= 0.1;
            } else {
                self.jumping = false;
                self.jump_offset = 0.0;
                self.jump_direction = true;
            }
        }
        if self.is_moving() {
            self.move_direction()
        } else if self.current_accel > PLAYER_STARTING_ACCEL {
            self.move_direction_cooldown()
        }
        if self.eats(food) {
            self.ate = Some(Ate::Potion);
        } else {
            self.ate = None
        }
    }

    fn get_animation_direction(&self) -> f32 {
        if self.dir.up {
            0.25
        } else if self.dir.left {
            0.5
        } else if self.dir.right {
            0.75
        } else if self.dir.down {
            0.0
        } else if self.last_dir.left {
            0.5
        } else if self.last_dir.right {
           0.75
        } else if self.last_dir.up {
            0.25
        } else {
            0.0
        }
    }

    fn animate_frames(&mut self) {
        // Animation movement
        if self.is_moving() && self.last_animation.elapsed() > self.animation_duration {
            self.last_animation = Instant::now();
            self.animation_frame += 1.0 / self.animation_total_frames;
            if self.animation_frame >= 1.0 {
                self.animation_frame = 0.0;
            }
        }
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult<()> {
        // And then we do the same for the head, instead making it fully red to distinguish it.
        //let bounding_box_rectangle = graphics::Mesh::new_rectangle(
        //    ctx,
        //    graphics::DrawMode::fill(),
        //    self.body.into(),
        //    [1.0, 0.5, 0.0, 1.0].into(),
        //)?;
        //graphics::draw(ctx, &bounding_box_rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))?;

        if self.jumping {
            let bounding_box_rectangle = graphics::Mesh::new_circle(
                ctx,
                graphics::DrawMode::fill(),
                ggez::mint::Point2 { x: self.body.x + 15.0,  y: self.body.y + 47.0 },
                14.0,
                1.0,
                graphics::Color::new(0.0, 0.0, 0.0, 0.3),
            )?;
            graphics::draw(ctx, &bounding_box_rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))?;
        }

        let black_rectangle = graphics::Mesh::new_rectangle(
            ctx,
            graphics::DrawMode::fill(),
            Rect::new(self.body.x - 13.0, self.body.y - 45.0, 60.0, 35.0),
            [0.0, 0.0, 0.0, 1.0].into(),
        )?;
        graphics::draw(ctx, &black_rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))?;

        let player_name = graphics::Text::new(graphics::TextFragment {
            text: format!("{}", self.name),
            color: Some(graphics::Color::new(1.0, 1.0, 1.0, 1.0)),
            // `Font` is a handle to a loaded TTF, stored inside the `Context`.
            // `Font::default()` always exists and maps to DejaVuSerif.
            font: Some(graphics::Font::default()),
            scale: Some(graphics::PxScale { x: 15.0, y: 15.0 }),
            ..Default::default()
        });
        let player_hp = graphics::Text::new(graphics::TextFragment {
            text: format!("{}", self.hp),
            color: Some(graphics::Color::new(0.9, 0.0, 0.0, 1.0)),
            // `Font` is a handle to a loaded TTF, stored inside the `Context`.
            // `Font::default()` always exists and maps to DejaVuSerif.
            font: Some(graphics::Font::default()),
            scale: Some(graphics::PxScale { x: 15.0, y: 15.0 }),
            ..Default::default()
        });
        let player_mp = graphics::Text::new(graphics::TextFragment {
            text: format!("{}", self.mp),
            color: Some(graphics::Color::new(0.0, 0.4, 1.0, 1.0)),
            // `Font` is a handle to a loaded TTF, stored inside the `Context`.
            // `Font::default()` always exists and maps to DejaVuSerif.
            font: Some(graphics::Font::default()),
            scale: Some(graphics::PxScale { x: 15.0, y: 15.0 }),
            ..Default::default()
        });
        graphics::queue_text(ctx, &player_name, ggez::mint::Point2 { x: self.body.x - (self.name.chars().count() as f32) + 5.0, y: self.body.y - GRID_CELL_SIZE - 10.0 }, None);
        graphics::queue_text(ctx, &player_hp, ggez::mint::Point2 { x: self.body.x - (GRID_CELL_SIZE / 2.0) + 5.0, y: self.body.y - GRID_CELL_SIZE + 5.0 }, None);
        graphics::queue_text(ctx, &player_mp, ggez::mint::Point2 { x: self.body.x - (GRID_CELL_SIZE / 2.0) + 45.0, y: self.body.y - GRID_CELL_SIZE + 5.0 }, None);
        graphics::draw_queued_text(
            ctx,
            graphics::DrawParam::new()
                .dest(ggez::mint::Point2 { x: 0.0, y: 0.0}),
                //.rotation(-0.5),
            None,
            graphics::FilterMode::Linear,
        )?;
        self.animate_frames();
        let param = graphics::DrawParam::new()
        .src(graphics::Rect {x: self.animation_frame, y: self.get_animation_direction(), w: 0.25, h: 0.25})
        .dest(Vec2::new(self.body.x + 2.0, self.body.y - 10.0))
        .offset(Vec2::new(0.15, self.jump_offset))
        .scale(Vec2::new(0.1, 0.1));
        //.rotation((time % cycle) as f32 / cycle as f32 * 6.28)
        //.offset(Vec2::new(150.0, 150.0));
        graphics::draw(ctx, &self.texture, param)?;

        Ok(())
    }
}

struct Hud {
}

impl Hud {

    fn new() -> Hud {
        Hud {}
    }

    fn draw(&self, ctx: &mut Context, player: &Player) -> GameResult<()> {
        let color = [0.0, 0.0, 0.0, 1.0].into();
        let top_back = graphics::Rect {
                x: 0.0,
                y: 0.0,
                w: 1000.0,
                h: GRID_CELL_SIZE,
        };
        let bottom_back = graphics::Rect {
                x: 0.0,
                y: SCREEN_SIZE.1 - GRID_CELL_SIZE,
                w: 1000.0,
                h: GRID_CELL_SIZE,
        };
        let top_rectangle =
            graphics::Mesh::new_rectangle(ctx, graphics::DrawMode::fill(), top_back, color)?;
        graphics::draw(ctx, &top_rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))?;
        let bottom_rectangle =
            graphics::Mesh::new_rectangle(ctx, graphics::DrawMode::fill(), bottom_back, color)?;
        graphics::draw(ctx, &bottom_rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))?;
        let player_name = graphics::Text::new(graphics::TextFragment {
                text: format!("Player: {}", player.name),
                color: Some(graphics::Color::new(1.0, 1.0, 1.0, 1.0)),
                // `Font` is a handle to a loaded TTF, stored inside the `Context`.
                // `Font::default()` always exists and maps to DejaVuSerif.
                font: Some(graphics::Font::default()),
                scale: Some(graphics::PxScale { x: 30.0, y: 30.0 }),
                ..Default::default()
            });
        let hp_text = graphics::Text::new(graphics::TextFragment {
                text: format!("{}", player.hp),
                color: Some(graphics::Color::new(1.0, 0.2, 0.2, 1.0)),
                // `Font` is a handle to a loaded TTF, stored inside the `Context`.
                // `Font::default()` always exists and maps to DejaVuSerif.
                font: Some(graphics::Font::default()),
                scale: Some(graphics::PxScale { x: 30.0, y: 30.0 }),
                ..Default::default()
            });
        let str_text = graphics::Text::new(graphics::TextFragment {
                text: format!("{}", player.str),
                color: Some(graphics::Color::new(1.0, 1.0, 0.2, 1.0)),
                // `Font` is a handle to a loaded TTF, stored inside the `Context`.
                // `Font::default()` always exists and maps to DejaVuSerif.
                font: Some(graphics::Font::default()),
                scale: Some(graphics::PxScale { x: 30.0, y: 30.0 }),
                ..Default::default()
            });
        let mp_text = graphics::Text::new(graphics::TextFragment {
                text: format!("{}", player.mp),
                color: Some(graphics::Color::new(0.0, 0.4, 1.0, 1.0)),
                // `Font` is a handle to a loaded TTF, stored inside the `Context`.
                // `Font::default()` always exists and maps to DejaVuSerif.
                font: Some(graphics::Font::default()),
                scale: Some(graphics::PxScale { x: 30.0, y: 30.0 }),
                ..Default::default()
            });
        graphics::queue_text(ctx, &str_text, ggez::mint::Point2 { x: 130.0, y: SCREEN_SIZE.1 - GRID_CELL_SIZE }, None);
        graphics::queue_text(ctx, &mp_text, ggez::mint::Point2 { x: 70.0, y: SCREEN_SIZE.1 - GRID_CELL_SIZE }, None);
        graphics::queue_text(ctx, &hp_text, ggez::mint::Point2 { x: 0.0, y: SCREEN_SIZE.1 - GRID_CELL_SIZE }, None);
        graphics::queue_text(ctx, &player_name, ggez::mint::Point2 { x: 0.0, y: 0.0 }, None);
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

#[derive(Debug)]
pub struct NetworkedGame {
    players: Vec<Player>,
    session_id: String,
}

impl NetworkedGame {

    pub fn new() -> NetworkedGame {
        let my_uuid = Uuid::new_v4().to_string();

        NetworkedGame {
            players: vec![],
            session_id: my_uuid,
        }
    }

}

pub struct GameServer {
    hostname: String,
    games: Vec<NetworkedGame>
}

impl GameServer {

    fn new(hostname: String) -> GameServer {
        GameServer {
            hostname,
            games: vec![],
        }
    }

    fn host(&mut self) {
        let listener = TcpListener::bind(self.hostname.as_str()).unwrap();

        for stream in listener.incoming() {
            let stream = stream.unwrap();
            self.handle_connection(stream)
        }
    }

    fn handle_connection(&mut self, mut stream: TcpStream) {
        let mut buffer = [0; PACKET_SIZE];
    
        stream.read(&mut buffer).unwrap();
        let request = String::from_utf8_lossy(&buffer[..]);
        println!("Received request: {}", request.to_string().as_str());

        if &request.to_string().as_str()[0..7] == "newgame" {
            let game = NetworkedGame::new();
            let _ = stream.write(format!("game_id: {}", game.session_id).as_bytes());
            self.games.push(game);
        } else if &request.to_string().as_str()[0..9] == "listgames" {
            let games = format!("Games: {:?}", self.games);
            let _ = stream.write(games.as_bytes());
        } else if &request.to_string().as_str()[0..8] == "gameinfo" {
            let game_id = &request.to_string().as_str()[8..44].to_string();
            if let Some(game) = self.games.iter().find(|g| &g.session_id == game_id) {
                let response = format!("game_info:  {:?}", game);
                let _ = stream.write(response.as_bytes());
            }
        } else {
            let _ = stream.write("Invalid Command".to_string().as_bytes());
        }
    }

    fn send_message(host: String, msg: String) {
        match TcpStream::connect(host.clone()) {
            Ok(mut stream) => {
                println!("Successfully connected to server {}", host);
    
                let msg = format!("{}", msg);
    
                stream.write(msg.as_bytes()).unwrap();
                println!("Sent {} awaiting reply...", msg);
    
                let mut data = [0 as u8; PACKET_SIZE]; 
                match stream.read(&mut data) {
                    Ok(_) => {
                        if &data[0..7] == "got it!".as_bytes() {
                            println!("Reply is {:?}", std::str::from_utf8(&data).unwrap());
                        } else if &data[0..7] == "game_id".as_bytes() {
                            let text = from_utf8(&data).unwrap();
                            println!("New game created: {}", text);
                        } else if &data[0..5] == "Games".as_bytes() {
                            let text = from_utf8(&data).unwrap();
                            println!("{}", text);
                        } else if &data[0..9] == "game_info".as_bytes() {
                            let text = from_utf8(&data).unwrap();
                            println!("{}", text);
                        } else {
                            let text = from_utf8(&data).unwrap();
                            println!("Unexpected reply: {}", text);
                        }
                    },
                    Err(e) => {
                        println!("Failed to receive data: {}", e);
                    }
                }
            },
            Err(e) => {
                println!("Failed to connect: {}", e);
            }
        }
    }

    fn new_game() -> NetworkedGame {
        NetworkedGame::new()
    }
}

struct GameState {
    /// First we need a Player
    player: Player,
    /// A piece of food
    food: Potion,
    /// Whether the game is over or not
    gameover: bool,
    /// And we track the last time we updated so that we can limit
    /// our update rate.
    last_update: Instant,
    hud: Hud,
    textures: HashMap<String, graphics::ImageGeneric<GlBackendSpec>>
}

impl GameState {

    fn connect_client(server: String, player: String) {
        let msg = format!("Hello server, it's {}!", player);
        GameServer::send_message(server, msg);
    }

    pub fn new(player_name: String, mut textures: HashMap<String, graphics::ImageGeneric<GlBackendSpec>>) -> Self {

        //std::thread::sleep(std::time::Duration::from_millis(1000));
        GameState::connect_client("localhost:7878".to_string(), player_name.clone());

        let mut rng = rand::thread_rng();
        let player_pos = Position { x: 100.0, y: 100.0 };
        let food_pos = Position { x: rng.gen_range(0, SCREEN_SIZE.0 as i16) as f32,
                                  y: rng.gen_range(0, SCREEN_SIZE.1 as i16) as f32 };
        let potion_texture = textures.remove("potion").unwrap();
        let player_texture = textures.remove("hero").unwrap();
        let player = Player::new(player_name.clone(), player_pos, player_texture);

        GameState {
            player: player,
            food: Potion::new(food_pos, potion_texture),
            hud: Hud::new(),
            gameover: false,
            last_update: Instant::now(),
            textures,
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
                        Ate::Potion => {
                            let mut rng = rand::thread_rng();
                            self.food.pos = Position { x: rng.gen_range(0, (SCREEN_SIZE.0 - GRID_CELL_SIZE) as i16) as f32,
                                                       y: rng.gen_range(0, (SCREEN_SIZE.1 - GRID_CELL_SIZE) as i16) as f32 }
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
        graphics::clear(ctx, [0.0, 0.5, 0.0, 1.0].into());
        let param = graphics::DrawParam::new()
        .dest(Vec2::new(0.0, 0.0));
        graphics::draw(ctx, self.textures.get("background").unwrap(), param)?;

        // <TODO Load Map> //

        // Then we tell the player and the items to draw themselves
        self.player.draw(ctx)?;
        self.food.draw(ctx)?;
        self.hud.draw(ctx, &self.player)?;


        graphics::present(ctx)?;
        ggez::timer::yield_now();
        Ok(())
    }

    fn key_up_event(
        &mut self,
        _ctx: &mut Context,
        keycode: KeyCode,
        _keymod: KeyMods,
    ) {
        match keycode {
            KeyCode::A => self.player.dir.left = false,
            KeyCode::D => self.player.dir.right = false,
            KeyCode::W => self.player.dir.up = false,
            KeyCode::S => self.player.dir.down = false,
            KeyCode::Escape => panic!("Escape!"),
            _ => ()
        };
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
            KeyCode::A => self.player.dir.left = true,
            KeyCode::D => self.player.dir.right = true,
            KeyCode::W => self.player.dir.up = true,
            KeyCode::S => self.player.dir.down = true,
            KeyCode::Space => self.player.jumping = true,
            _ => ()
        };
    }
}

fn main() -> GameResult {

    let matches = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg("-s --server=[HOSTNAME:PORT] 'Set as server and assign hostname:port'")
        .arg("-l --list=[HOSTNAME:PORT] 'List all games on server'")
        .get_matches();

    // if hosting
    if let Some(server) = matches.clone().value_of("server") {
        let safe_server = server.clone().to_string();
        std::thread::spawn(move || {
            let mut gameserver = GameServer::new(safe_server);
            gameserver.host();
        });
        let mut server_input = String::new();
        println!("Started Item Wars Server on {}", server);
        let mut current_games: Vec<NetworkedGame> = vec![];
        loop {
            server_input = "".to_string();
            println!("\nITEM WARS ENTER COMMAND :> ");
            let _ = io::stdin().read_line(&mut server_input);
            server_input.retain(|c| !c.is_whitespace());

            let command = server_input.to_ascii_lowercase().to_string();
            if command == "exit" {
                panic!("Exit");
            }
            GameServer::send_message(server.clone().to_string(), command);
        }
        Ok(())
    } else if let Some(list) = matches.clone().value_of("list") {
       GameServer::send_message(list.clone().to_string(), "listgames".to_string());
       Ok(())
    } else {
        let input = "Fred".to_string();

        let resource_dir = if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
            let mut path = path::PathBuf::from(manifest_dir);
            path.push("textures");
            path
        } else {
            path::PathBuf::from("./textures")
        };

        let (mut ctx, events_loop) = ggez::ContextBuilder::new("iterm wars", "Mitt Miles")
            .window_setup(ggez::conf::WindowSetup::default().title("Item Wars!"))
            .window_mode(ggez::conf::WindowMode::default().dimensions(SCREEN_SIZE.0, SCREEN_SIZE.1))
            .add_resource_path(resource_dir)
            .build()?;
        // To enable fullscreen
        //graphics::set_fullscreen(&mut ctx, ggez::conf::FullscreenType::True).unwrap();

        // Load our textures
        let mut textures: HashMap<String, ImageGeneric<GlBackendSpec>> = HashMap::new();
        textures.insert("background".to_string(), graphics::Image::new(&mut ctx, "/tile.png").unwrap());
        textures.insert("hero".to_string(), graphics::Image::new(&mut ctx, "/hero.png").unwrap());
        textures.insert("potion".to_string(), graphics::Image::new(&mut ctx, "/potion.png").unwrap());

        // Next we create a new instance of our GameState struct, which implements EventHandler
        let state = GameState::new(input, textures);
        // And finally we actually run our game, passing in our context and state.
        event::run(ctx, events_loop, state)
    }
}