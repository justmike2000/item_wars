//! Author: @justmike2000
//! Repo: https://github.com/justmike2000/item_wars/

use ggez::{event::{KeyCode, KeyMods}, filesystem::{open, resources_dir}};
use ggez::{event, graphics, Context, GameResult, timer};
use graphics::{GlBackendSpec, ImageGeneric, Rect};
use glam::*;

use std::{char::MAX, intrinsics::transmute, time::{Duration, Instant}};
use std::io;
use std::path;
use std::env;
use std::collections::HashMap;
use std::io::prelude::*;
use std::net::{UdpSocket, ToSocketAddrs};
use std::io::{Read, Write};
use std::str::from_utf8;

use serde::{Deserialize, Serialize};
use clap::{Arg, App};
use rand::Rng;
use uuid::Uuid;
use serde_json::{Result, Value, json, *};

// The first thing we want to do is set up some constants that will help us out later.

const SCREEN_SIZE: (f32, f32) = (640.0, 480.0);
const GRID_CELL_SIZE: f32 = 32.0;

const MAX_PLAYERS: usize = 2;

const PLAYER_MAX_HP: i64 = 100;
const PLAYER_MAX_MP: i64 = 30;
const PLAYER_MAX_STR: i64 = 10;
const PLAYER_MOVE_SPEED: f32 = 3.0;
const PLAYER_TOP_ACCEL_SPEED: f32 = 5.0;
const PLAYER_ACCEL_SPEED: f32 = 0.2;
const PLAYER_STARTING_ACCEL: f32 = 0.4;
const PLAYER_JUMP_HEIGHT: f32 = 0.5;
const PLAYER_CELL_HEIGHT: f32 = 44.0;
const PLAYER_CELL_WIDTH: f32 = 34.0;

const POTION_WIDTH: f32 = 42.0;
const POTION_HEIGHT: f32 = 42.0;

const NET_GAME_START_CHECK_MILLIS: u64 = 5000;

const MAP_CURRENT_FRICTION: f32 = 5.0;

const PACKET_SIZE: usize = 1_000;

const UPDATES_PER_SECOND: f32 = 30.0;
const DRAW_MILLIS_PER_UPDATE: u64 = (1.0 / UPDATES_PER_SECOND * 1000.0) as u64;
const NET_MILLIS_PER_UPDATE: u64 = 20;

const SERVER_PORT: i32 = 7878;
const SEND_PORT: i32 = 0;

#[derive(PartialOrd, Clone, Copy, Debug, Serialize, Deserialize)]
struct Position {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl From<Position> for Rect {
    fn from(pos: Position) -> Self {
        Rect { x: pos.x, y: pos.y, w: pos.w, h: pos.h }
    }
}

impl PartialEq for Position {
    fn eq(&self, other: &Self) -> bool {
        Rect::from(*self).overlaps(&Rect::from(*other))
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
struct Direction {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
enum PotionType {
    Health,
    Mana
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Potion {
    pos: Position,
    potion_type: PotionType,
    #[serde(skip_serializing, skip_deserializing)]
    texture: Option<ImageGeneric<GlBackendSpec>>,
}

impl Potion {

    pub fn new(pos: Position, potion_type: PotionType, texture: ImageGeneric<GlBackendSpec>) -> Self {
        Potion {
            pos,
            potion_type,
            texture: Some(texture),
        }
    }

    fn draw(&self, ctx: &mut Context) -> GameResult<()> {

        //let black_rectangle = graphics::Mesh::new_rectangle(
        //    ctx,
        //    graphics::DrawMode::fill(),
        //    Rect::new(self.pos.x, self.pos.y, self.pos.w, self.pos.h),
        //    [0.0, 0.0, 0.0, 1.0].into(),
        //)?;
        //graphics::draw(ctx, &black_rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))?;

        //let color = [0.0, 0.0, 1.0, 1.0].into();
        //let rectangle =
        //    graphics::Mesh::new_rectangle(ctx, graphics::DrawMode::fill(), self.pos.into(), color)?;
        //graphics::draw(ctx, &rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))
        let potion_frame = if self.potion_type == PotionType::Health {
            0.0
        } else if self.potion_type == PotionType::Mana {
            0.33
        } else {
            0.0
        };
        let param = graphics::DrawParam::new()
        .src(graphics::Rect {x: 0.0, y: potion_frame, w: 0.33, h: 0.33})
        .dest(Vec2::new(self.pos.x, self.pos.y))
        //.offset(Vec2::new(0.15, 0.0))
        .scale(Vec2::new(0.25, 0.25));
        //.rotation((time % cycle) as f32 / cycle as f32 * 6.28)
        //.offset(Vec2::new(150.0, 150.0));
        graphics::draw(ctx, &self.texture.clone().unwrap(), param)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Player {
    /// First we have the body of the player, which is a single `Segment`.
    body: Position,
    /// Then we have the current direction the player is moving. This is
    /// the direction it will move when `update` is called on it.
    dir: Direction,
    last_dir: Direction,
    ate: Option<Potion>,
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
    #[serde(skip_serializing, skip_deserializing)]
    texture: Option<ImageGeneric<GlBackendSpec>>,
    animation_frame: f32,
    animation_total_frames: f32,
    #[serde(skip_serializing, skip_deserializing)]
    last_animation: Option<std::time::Instant>,
    animation_duration: std::time::Duration,
}

impl Player {
    pub fn new(name: String, pos: Position, texture: Option<ImageGeneric<GlBackendSpec>>) -> Self {
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
            last_animation: Some(std::time::Instant::now()),
            animation_duration:  Duration::new(0, 150_000_000),
        }
    }

    fn eats(&self, potion: &Potion) -> bool {
        if self.body == potion.pos {
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

    fn update(&mut self) {
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
        //if self.eats(food) && !self.jumping {
        //    self.ate = Some(food.clone());
        //} else {
        //    self.ate = None
        //}
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
        if self.is_moving() && self.last_animation.unwrap().elapsed() > self.animation_duration {
            self.last_animation = Some(Instant::now());
            self.animation_frame += 1.0 / self.animation_total_frames;
            if self.animation_frame >= 1.0 {
                self.animation_frame = 0.0;
            }
        }
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult<()> {
        if let Some(ate) = &self.ate {
            println!("{:?}", ate.pos);
        }
        // And then we do the same for the head, instead making it fully red to distinguish it.
        //let bounding_box_rectangle = graphics::Mesh::new_rectangle(
        //    ctx,
        //    graphics::DrawMode::fill(),
        //    self.body.into(),
        //    [1.0, 0.5, 0.0, 1.0].into(),
        //)?;
        //graphics::draw(ctx, &bounding_box_rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))?;
        //let black_rectangle = graphics::Mesh::new_rectangle(
        //    ctx,
        //    graphics::DrawMode::fill(),
        //    Rect::new(self.body.x, self.body.y, self.body.w, self.body.h),
        //    [0.0, 0.0, 0.0, 1.0].into(),
        //)?;
        //graphics::draw(ctx, &black_rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))?;

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
        if let Some(player_texture) = &self.texture {
            graphics::draw(ctx, player_texture, param)?;
        }
        Ok(())
    }
}

#[derive(Clone)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkedGame {
    players: Vec<Player>,
    session_id: String,
    started: bool,
    completed: bool,
}

impl NetworkedGame {

    pub fn new() -> NetworkedGame {
        let my_uuid = Uuid::new_v4().to_string();

        NetworkedGame {
            players: vec![],
            session_id: my_uuid,
            started: false,
            completed: false
        }
    }

}

pub struct GameServer {
    hostname: String,
    games: Vec<NetworkedGame>,
}

impl GameServer {

    fn new(hostname: String) -> GameServer {
        GameServer {
            hostname,
            games: vec![],
        }
    }

    fn host(&mut self) {
        let addr = format!("{}:{}", self.hostname.clone(), SERVER_PORT); 
        let listener = UdpSocket::bind(addr).unwrap();
        listener.set_nonblocking(true).unwrap();
        listener.set_broadcast(true).unwrap();
        listener.set_read_timeout(Some(Duration::new(5, 0))).unwrap();

        let mut buf = [0; PACKET_SIZE];
        loop {
           match listener.recv_from(&mut buf) {
               Ok((amt, src)) => {
                   let request = String::from_utf8_lossy(&buf[..]);
                   self.handle_connection(request.to_string(), amt, src.to_string(), &listener);
               },
               Err(e) => {
                   //println!("couldn't recieve a datagram: {}", e);
               }
           }
        }
    }

    fn handle_connection(&mut self, mut request: String, amt: usize, dst: String, socket: &UdpSocket) {
        let parsed_request: serde_json::Value = match serde_json::from_str(&request[..amt]) {
            Ok(r) => r,
            Err(e) => {
                println!("Invalid request {} - {}", request, e);
                return 
            }
        };
        //println!("Received request: {}", string_request);

        let data = match parsed_request["command"].as_str() {
            Some("newgame") => {
                let game = NetworkedGame::new();
                self.games.push(game.clone());
                json!({
                    "game_id": game.session_id,
                })
            },
            Some("listgames") => {
                let game_info: Vec<Vec<String>> = self.games.iter().filter(|game| !game.started ).map(|game| {
                    vec![game.session_id.clone(), game.players.len().to_string()]
                }).collect();
                json!({
                    "games": game_info ,
                })
            },
            Some("joingame") => {
                let game_id = parsed_request["game_id"].as_str().unwrap_or("");
                if let Some(game) = self.games.iter_mut().find(|g| &g.session_id == game_id) {
                    if game.players.len() < MAX_PLAYERS {
                        let player_pos = Position { x: 0.0, y: 0.0, w: PLAYER_CELL_WIDTH, h: PLAYER_CELL_HEIGHT };
                        let new_player = Player::new(parsed_request["name"].as_str().unwrap_or("").to_string(), player_pos, None);
                        game.players.push(new_player);
                        if game.players.len() == MAX_PLAYERS {
                            println!("Starting game {}", game.session_id);
                            game.started = true;
                        }
                        let started_string = match game.started {
                            true => "started",
                            false => "not started",
                        };
                        json!({"info": format!("joined {} game {} with {} players", started_string, game.session_id, game.players.len())})
                    } else {
                        json!({"error": format!("game {:?} is full", game.session_id)})
                    }
                } else {
                    json!({"error": format!("Invalid Game {}", game_id)})
                }
            },
            Some("gameinfo") => {
                let game_id = parsed_request["game_id"].as_str().unwrap_or("");
                if let Some(game) = self.games.iter().find(|g| &g.session_id == game_id) {
                    json!({"game": vec![game.session_id.clone(), game.players.len().to_string()]})
                } else {
                    json!({"error": format!("Invalid Game {}", game_id)})
                }
            },
            Some("sendposition") => {
                let game_id = parsed_request["game_id"].as_str().unwrap_or("");
                if let Some(game) = self.games.iter_mut().find(|g| &g.session_id == game_id) {
                    let name = parsed_request["name"].as_str().unwrap_or("");
                    if let Some(player) = game.players.iter_mut().find(|p| &p.name == name) {
                        let update_player: Player = serde_json::from_str::<Player>(parsed_request["meta"].as_str().unwrap()).unwrap();
                        *player = update_player;
                    }
                    json!(game)
                } else {
                    json!({"error": format!("Invalid Game {}", game_id)})
                }
            },
            Some("getworld") => {
                let game_id = parsed_request["game_id"].as_str().unwrap_or("");
                if let Some(game) = self.games.iter().find(|g| &g.session_id == game_id) {
                    json!(game)
                } else {
                    json!({"error": format!("Invalid Game {}", game_id)})
                }
            },
            _ => {
                json!({
                    "error": "Invalid Command",
                })
            }
        };
        socket.send_to(data.to_string().as_bytes(), dst.clone());
    }

    fn send_message(host: String, game_id: String, player: String, msg: String, meta: String) -> String {
        let addr = format!("{}:{}", host, SEND_PORT);
        let socket = UdpSocket::bind(addr).unwrap();

        //println!("Successfully connected to server {}", host);
    
        let data = json!({
            "game_id": game_id.clone(),
            "name": player.clone(),
            "command": msg.clone(),
            "meta": meta.clone(),
        });
        let msg = data.to_string();
    
        let server = format!("{}:{}", host.clone(), SERVER_PORT);
        socket.send_to(msg.as_bytes(), server);
        //println!("Sent {} awaiting reply...", msg);
    
        let mut data = [0 as u8; PACKET_SIZE]; 
        match socket.recv_from(&mut data) {
            Ok((amt, _)) => String::from_utf8_lossy(&data)[0..amt].to_string(),
            Err(e) => {
                format!("Failed to connect: {}", e)
            }
        }
    }

    fn new_game() -> NetworkedGame {
        NetworkedGame::new()
    }
}

#[derive(Clone)]
struct GameState {
    player: Player,
    opponent: Player,
    food: Potion,
    server: String,
    game_id: String,
    started: bool,
    gameover: bool,
    last_draw_update: Instant,
    last_net_update: Instant,
    hud: Hud,
    textures: HashMap<String, graphics::ImageGeneric<GlBackendSpec>>,
}

impl GameState {

    fn join_game(server: String, player: String, game_id: String) {
        let msg = format!("joingame");
        let result = GameServer::send_message(server, game_id, player, msg, "".to_string());
        println!("{}", result);
    }

    fn get_world_state(server: String, player: String, game_id: String) -> NetworkedGame {
        let msg = format!("getworld");
        let result = GameServer::send_message(server, game_id, player, msg, "".to_string());
        serde_json::from_str(&result).unwrap()
    }

    fn send_position(server: String, player: Player, game_id: String) {
        GameServer::send_message(server, game_id, player.name.clone(), "sendposition".to_string(), json!(player).to_string());
    }

    pub fn new<'a>(player_name: String, host: String, game_id: String ,mut textures: HashMap<String, graphics::ImageGeneric<GlBackendSpec>>) -> Self {

        let game_server = GameServer::new(host.clone());
        //std::thread::sleep(std::time::Duration::from_millis(1000));
        GameState::join_game(host.clone(), player_name.clone(), game_id.clone());

        let mut rng = rand::thread_rng();
        let player_pos = Position { x: 100.0, y: 100.0, w: PLAYER_CELL_WIDTH, h: PLAYER_CELL_HEIGHT };
        let food_pos = Position { x: rng.gen_range(0, SCREEN_SIZE.0 as i16) as f32,
                                           y: rng.gen_range(0, SCREEN_SIZE.1 as i16) as f32,
                                           w: POTION_WIDTH,
                                           h: POTION_HEIGHT };
        let potion_texture = textures.remove("potion").unwrap();
        let player_texture = textures.remove("hero").unwrap();
        let player = Player::new(player_name.clone(), player_pos, Some(player_texture.clone()));
        let opponent = Player::new(player_name.clone(), player_pos, Some(player_texture.clone()));

        GameState {
            player: player,
            opponent: opponent,
            server: host.clone(),
            game_id: game_id.clone(),
            food: Potion::new(food_pos, PotionType::Health, potion_texture),
            hud: Hud::new(),
            gameover: false,
            started: false,
            last_draw_update: Instant::now(),
            last_net_update: Instant::now(),
            textures,
        }
    }
}

impl event::EventHandler for GameState {
    fn update(&mut self, _ctx: &mut Context) -> GameResult {
        if !self.started {
            if Instant::now() - self.last_net_update >= Duration::from_millis(NET_GAME_START_CHECK_MILLIS) {
                let get_world = GameState::get_world_state(self.server.clone(), self.player.name.clone(), self.game_id.clone());
                if !get_world.started {
                    println!("Waiting for game {} to start...", self.game_id.clone());
                    self.last_net_update = Instant::now();
                    return Ok(())
                } else {
                    println!("Game started!");
                    if let Some(opponent) = get_world.players.iter().find(|p| p.name != self.player.name) {
                        self.opponent.name = opponent.name.clone();
                        self.opponent.body = opponent.body;
                        self.opponent.dir = opponent.dir.clone();
                        self.opponent.last_dir = opponent.last_dir.clone();
                        self.opponent.jumping = opponent.jumping;
                    }
                    self.started = true
                }
            } else {
                return Ok(())
            }
        } 

        if Instant::now() - self.last_draw_update >= Duration::from_millis(DRAW_MILLIS_PER_UPDATE) {
            if !self.gameover {
                self.player.update();

                if Instant::now() - self.last_net_update >= Duration::from_millis(NET_MILLIS_PER_UPDATE) {
                    GameState::send_position(self.server.clone(), self.player.clone(), self.game_id.clone());
                    let get_world = GameState::get_world_state(self.server.clone(), self.player.name.clone(), self.game_id.clone());
                    if let Some(opponent) = get_world.players.iter().find(|p| p.name != self.player.name) {
                        self.opponent.name = opponent.name.clone();
                        self.opponent.body = opponent.body;
                        self.opponent.dir = opponent.dir.clone();
                        self.opponent.last_dir = opponent.last_dir.clone();
                        self.opponent.jumping = opponent.jumping;
                        self.opponent.update();
                    }
                    self.last_net_update = Instant::now();
                }
                //if let Some(ate) = &self.player.ate {
                //        let mut rng = rand::thread_rng();
                //        self.food.pos = Position { x: rng.gen_range(GRID_CELL_SIZE as i16, (SCREEN_SIZE.0 - POTION_WIDTH) as i16) as f32,
                //                                   y: rng.gen_range(GRID_CELL_SIZE as i16, (SCREEN_SIZE.1 - POTION_WIDTH) as i16) as f32 ,
                //                                   w: POTION_WIDTH,
                //                                   h: POTION_HEIGHT };
                //}
            }
            self.last_draw_update = Instant::now();
        }
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        graphics::clear(ctx, [0.0, 0.5, 0.0, 1.0].into());
        let param = graphics::DrawParam::new()
        .dest(Vec2::new(0.0, 0.0));
        graphics::draw(ctx, self.textures.get("background").unwrap(), param)?;

        // <TODO Load Map> //

        // Then we tell the player and the items to draw themselves
        self.player.draw(ctx)?;
        self.opponent.draw(ctx)?;
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
        .arg("-h --host=[HOSTNAME:PORT] 'Set as server and assign hostname:port'")
        .arg("-l --list=[HOSTNAME:PORT] 'List all games on server'")
        .arg("-p --player=[NAME] 'Player Name'")
        .arg("-s --server=[HOSTNAME:PORT] 'Host to connect to'")
        .arg("-g --game=[GAMEID] 'GameID to join'")
        .get_matches();

    // if hosting
    if let Some(server) = matches.clone().value_of("host") {
        let safe_server = server.clone().to_string();
        std::thread::spawn(move || {
            let mut gameserver = GameServer::new(safe_server);
            gameserver.host();
        });
        let mut server_input = String::new();
        println!("Started Item Wars Server on {}", server);
        let mut current_games: Vec<NetworkedGame> = vec![];
        let mut player = "".to_string();
        let mut game_id = "".to_string();
        loop {
            server_input = "".to_string();
            println!("\nITEM WARS ENTER COMMAND :> ");
            let _ = io::stdin().read_line(&mut server_input);
            server_input.retain(|c| !c.is_whitespace());

            let command = server_input.to_ascii_lowercase().to_string();
            if command.len() >= 7 && command[0..7].to_string() == "setgame" {
                game_id = command[7..].to_string();
                println!("Game ID set to {}", game_id);
            } else if command.len() >= 9 && command[0..9].to_string() == "setplayer" {
                player = command[9..].to_string();
                println!("Playername set to {}", player);
            } else if command == "exit" {
                panic!("Exit");
            } else {
                let result = GameServer::send_message(server.clone().to_string(),
                                                           game_id.clone(), player.to_string(), command, "".to_string());
                println!("{}", result);
                if let Ok(result_obj) = serde_json::from_str::<serde_json::Value>((&result)) {
                    if let Some(new_game_id) = result_obj["game_id"].as_str() {
                        game_id = new_game_id.to_string();
                        println!("Game ID set to {}", game_id);
                    }
                }
            }
        }
        Ok(())
    } else if let Some(list) = matches.clone().value_of("list") {
       let games = GameServer::send_message(list.clone().to_string(), "".to_string(), "".to_string(), "listgames".to_string(), "".to_string());
       println!("{:?}", games);
       Ok(())
    } else {
        let player_name = matches.clone().value_of("player").unwrap_or("Player").to_string();
        let host = matches.clone().value_of("server").unwrap_or("localhost:7878").to_string();
        let game_id = match matches.clone().value_of("game") {
            Some(g ) => g.to_string(),
            None => {
                panic!("Please provide gameid.")
            },
        };

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
        let state = GameState::new(player_name, host, game_id, textures);
        // And finally we actually run our game, passing in our context and state.
        event::run(ctx, events_loop, state)
    }
}