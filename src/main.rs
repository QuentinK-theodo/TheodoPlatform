//! Theodo RPG — a tiny Mario & Luigi–style roguelite demo for the GBA.
#![no_std]
#![no_main]
#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, reexport_test_harness_main = "test_main")]
#![cfg_attr(test, test_runner(agb::test_runner::test_runner))]

extern crate alloc;

mod wfc;

use alloc::{format, string::String, vec::Vec};

use agb::{
    display::{
        GraphicsFrame, Palette16, Priority, Rgb15,
        font::{Font, Layout, LayoutSettings, ObjectTextRenderer},
        object::{Object, Size, Tag},
        tiled::{RegularBackground, RegularBackgroundSize, TileFormat},
    },
    fixnum::vec2,
    include_aseprite, include_background_gfx, include_font,
    input::{Button, ButtonController},
};

use wfc::{Map, Rng, Terrain};

include_background_gfx!(mod bg, tiles => "gfx/tiles.png");
include_aseprite!(
    mod sprites,
    "gfx/adam_down.gif",
    "gfx/adam_up.gif",
    "gfx/adam_left.gif",
    "gfx/adam_right.gif",
    "gfx/adam_idle.gif",
    "gfx/bob_idle.gif",
    "gfx/alex_idle.gif"
);

static FONT: Font = include_font!("fonts/ark-pixel-10px-proportional-ja.ttf", 10);

static TEXT_PALETTE: &Palette16 = const {
    let mut palette = [Rgb15::BLACK; 16];
    palette[1] = Rgb15::WHITE;
    &Palette16::new(palette)
};

const HERO_MAX_HP: i32 = 20;

// --- Text ----------------------------------------------------------------

struct Text {
    objects: Vec<Object>,
}

impl Text {
    fn new(s: &str, x: i32, y: i32) -> Self {
        let renderer = ObjectTextRenderer::new(TEXT_PALETTE.into(), Size::S16x16);
        let layout = Layout::new(
            s,
            &FONT,
            &LayoutSettings::new()
                .with_max_line_length(232)
                .with_drop_shadow(2),
        );
        Text {
            objects: layout.map(|l| renderer.show(&l, vec2(x, y))).collect(),
        }
    }
    fn show(&self, frame: &mut GraphicsFrame) {
        for o in &self.objects {
            o.show(frame);
        }
    }
}

// --- World ----------------------------------------------------------------

#[derive(Clone, Copy)]
enum Kind {
    Bob,
    Alex,
}

impl Kind {
    fn tag(self) -> &'static Tag {
        match self {
            Kind::Bob => &sprites::BOB_IDLE,
            Kind::Alex => &sprites::ALEX_IDLE,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Kind::Bob => "Bob",
            Kind::Alex => "Alex",
        }
    }
}

struct Enemy {
    kind: Kind,
    x: i32,
    y: i32,
    hp: i32,
}

struct Room {
    map: Map,
    enemies: Vec<Enemy>,
    hero: (i32, i32),
}

fn new_room(rng: &mut Rng, depth: i32) -> Room {
    let mut map = wfc::generate(rng);
    let walkable: Vec<(usize, usize)> = (0..wfc::H)
        .flat_map(|y| (0..wfc::W).map(move |x| (x, y)))
        .filter(|&(x, y)| !map[y][x].blocks())
        .collect();
    let start = if walkable.is_empty() {
        map[5][7] = Terrain::Floor;
        (7, 5)
    } else {
        walkable[rng.range(walkable.len() as u32) as usize]
    };
    let mut reachable = wfc::make_connected(&mut map, start);

    // Spawn enemies away from the hero.
    let far: Vec<(usize, usize)> = reachable
        .iter()
        .copied()
        .filter(|&(x, y)| x.abs_diff(start.0) + y.abs_diff(start.1) >= 4)
        .collect();
    if far.len() >= 3 {
        reachable = far;
    }
    let mut enemies = Vec::new();
    for i in 0..3 {
        if reachable.len() <= 1 {
            break;
        }
        let idx = 1 + rng.range(reachable.len() as u32 - 1) as usize;
        let (x, y) = reachable.swap_remove(idx);
        enemies.push(Enemy {
            kind: if i % 2 == 0 { Kind::Bob } else { Kind::Alex },
            x: x as i32 * 16,
            y: y as i32 * 16 - 16,
            hp: 6 + depth * 2 + rng.range(5) as i32,
        });
    }

    Room {
        map,
        enemies,
        hero: (start.0 as i32 * 16, start.1 as i32 * 16 - 16),
    }
}

fn draw_map(background: &mut RegularBackground, map: &Map) {
    for y in 0..wfc::H {
        for x in 0..wfc::W {
            let t = map[y][x] as usize;
            for dy in 0..2 {
                for dx in 0..2 {
                    let idx = dy * 12 + t * 2 + dx;
                    background.set_tile(
                        (x as i32 * 2 + dx as i32, y as i32 * 2 + dy as i32),
                        &bg::tiles.tiles,
                        bg::tiles.tile_settings[idx],
                    );
                }
            }
        }
    }
}

fn blocked(map: &Map, px: i32, py: i32) -> bool {
    // Feet hitbox: 10x6 at the bottom of the 16x32 sprite.
    for (fx, fy) in [(3, 26), (12, 26), (3, 31), (12, 31)] {
        let (cx, cy) = ((px + fx) / 16, (py + fy) / 16);
        if cx < 0 || cy < 0 || cx >= wfc::W as i32 || cy >= wfc::H as i32 {
            return true;
        }
        if map[cy as usize][cx as usize].blocks() {
            return true;
        }
    }
    false
}

// --- Combat ---------------------------------------------------------------

enum Phase {
    /// Cursor sweeps; press A in the green zone for double damage.
    Aim { cursor: i32 },
    /// Enemy charges at the hero; press A at impact to dodge.
    EnemyAttack { t: i32, pressed: Option<i32> },
    Message { text: String, timer: i32, then: Then },
}

#[derive(Clone, Copy)]
enum Then {
    EnemyTurn,
    PlayerTurn,
    Victory,
    Defeat,
}

const BAR_X: i32 = 70;
const BAR_W: i32 = 100;
const SWEET_START: i32 = 42;
const SWEET_END: i32 = 58;
const ATTACK_LEN: i32 = 60;
const DODGE_FROM: i32 = 44;
const DODGE_TO: i32 = 56;

// --- Game -----------------------------------------------------------------

enum Mode {
    Explore,
    Combat { enemy: usize, phase: Phase },
}

#[agb::entry]
fn main(mut gba: agb::Gba) -> ! {
    let mut gfx = gba.graphics.get();
    let mut input = ButtonController::new();
    gfx.set_background_palettes(bg::PALETTES);

    let mut background = RegularBackground::new(
        Priority::P3,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );

    // Title screen: seed comes from how long the player waits.
    let title = Text::new(
        "THEODO RPG\n\nUn RPG roguelite dans les bureaux.\nCombat au tour par tour, A au bon moment !\n\nAppuie sur START",
        12,
        30,
    );
    let mut seed: u32 = 1;
    loop {
        input.update();
        seed = seed.wrapping_add(1);
        if input.is_just_pressed(Button::Start) || input.is_just_pressed(Button::A) {
            break;
        }
        let mut frame = gfx.frame();
        title.show(&mut frame);
        frame.commit();
    }

    let mut rng = Rng::new(seed);
    let mut depth = 1;
    let mut hero_hp = HERO_MAX_HP;
    let mut room = new_room(&mut rng, depth);
    draw_map(&mut background, &room.map);

    let mut hero = Object::new(sprites::ADAM_DOWN.sprite(0));
    let mut facing: &'static Tag = &sprites::ADAM_DOWN;
    let mut mode = Mode::Explore;
    let mut frame_count: usize = 0;
    let mut hud = Text::new(&format!("Salle {depth}   PV {hero_hp}/{HERO_MAX_HP}"), 4, 0);
    let mut hud_dirty = false;
    let mut combat_text: Option<Text> = None;
    let cursor_text = Text::new("v", 0, 0);
    let bar_text = Text::new("[==========]", BAR_X - 4, 104);

    loop {
        frame_count = frame_count.wrapping_add(1);
        input.update();

        match &mut mode {
            Mode::Explore => {
                let (dx, dy) = (input.x_tri() as i32, input.y_tri() as i32);
                if dx != 0 || dy != 0 {
                    facing = match (dx, dy) {
                        (1, _) => &sprites::ADAM_RIGHT,
                        (-1, _) => &sprites::ADAM_LEFT,
                        (_, -1) => &sprites::ADAM_UP,
                        _ => &sprites::ADAM_DOWN,
                    };
                    let speed = 2;
                    if !blocked(&room.map, room.hero.0 + dx * speed, room.hero.1) {
                        room.hero.0 += dx * speed;
                    }
                    if !blocked(&room.map, room.hero.0, room.hero.1 + dy * speed) {
                        room.hero.1 += dy * speed;
                    }
                    hero.set_sprite(facing.animation_sprite(frame_count / 6));
                } else {
                    hero.set_sprite(facing.sprite(0));
                }
                hero.set_pos((room.hero.0, room.hero.1));

                if let Some(i) = room.enemies.iter().position(|e| {
                    (e.x - room.hero.0).abs() < 14 && (e.y - room.hero.1).abs() < 14
                }) {
                    let e = &room.enemies[i];
                    combat_text = Some(Text::new(
                        &format!("{} attaque ! A pour frapper au bon moment.", e.kind.name()),
                        4,
                        130,
                    ));
                    mode = Mode::Combat {
                        enemy: i,
                        phase: Phase::Aim { cursor: 0 },
                    };
                    hud_dirty = true;
                }
            }
            Mode::Combat { enemy, phase } => {
                let e = &mut room.enemies[*enemy];
                let mut next: Option<Phase> = None;
                match phase {
                    Phase::Aim { cursor } => {
                        *cursor += 3;
                        let pressed = input.is_just_pressed(Button::A);
                        if pressed || *cursor >= BAR_W {
                            let (dmg, msg) = if pressed && (SWEET_START..=SWEET_END).contains(cursor) {
                                (8, "EXCELLENT ! 8 degats")
                            } else if pressed {
                                (4, "Touche. 4 degats")
                            } else {
                                (2, "Trop lent... 2 degats")
                            };
                            e.hp -= dmg;
                            next = Some(Phase::Message {
                                text: msg.into(),
                                timer: 50,
                                then: if e.hp <= 0 { Then::Victory } else { Then::EnemyTurn },
                            });
                        }
                    }
                    Phase::EnemyAttack { t, pressed } => {
                        *t += 1;
                        if pressed.is_none() && input.is_just_pressed(Button::A) {
                            *pressed = Some(*t);
                        }
                        if *t >= ATTACK_LEN {
                            let dodged = matches!(pressed, Some(p) if (DODGE_FROM..=DODGE_TO).contains(p));
                            let dmg = if dodged { 0 } else { 2 + depth };
                            hero_hp -= dmg;
                            next = Some(Phase::Message {
                                text: if dodged {
                                    "ESQUIVE !".into()
                                } else {
                                    format!("Aie ! -{dmg} PV")
                                },
                                timer: 50,
                                then: if hero_hp <= 0 { Then::Defeat } else { Then::PlayerTurn },
                            });
                        }
                    }
                    Phase::Message { timer, then, .. } => {
                        *timer -= 1;
                        if *timer <= 0 {
                            match *then {
                                Then::EnemyTurn => {
                                    next = Some(Phase::EnemyAttack { t: 0, pressed: None });
                                    combat_text = Some(Text::new("A au moment de l'impact pour esquiver !", 4, 130));
                                }
                                Then::PlayerTurn => {
                                    next = Some(Phase::Aim { cursor: 0 });
                                    combat_text = Some(Text::new("A dans la zone centrale !", 4, 130));
                                }
                                Then::Victory => {
                                    room.enemies.swap_remove(*enemy);
                                    combat_text = None;
                                    if room.enemies.is_empty() {
                                        depth += 1;
                                        hero_hp = (hero_hp + 5).min(HERO_MAX_HP);
                                        room = new_room(&mut rng, depth);
                                        draw_map(&mut background, &room.map);
                                    }
                                    mode = Mode::Explore;
                                    hud_dirty = true;
                                    // Step away so we don't re-trigger instantly.
                                    continue_frame(&mut gfx, &background, &hero, &room, &hud, frame_count);
                                    continue;
                                }
                                Then::Defeat => {
                                    depth = 1;
                                    hero_hp = HERO_MAX_HP;
                                    room = new_room(&mut rng, depth);
                                    draw_map(&mut background, &room.map);
                                    combat_text = None;
                                    mode = Mode::Explore;
                                    hud_dirty = true;
                                    continue_frame(&mut gfx, &background, &hero, &room, &hud, frame_count);
                                    continue;
                                }
                            }
                        }
                    }
                }
                if let Some(p) = next {
                    if let Phase::Message { text, .. } = &p {
                        combat_text = Some(Text::new(text, 4, 130));
                    }
                    *phase = p;
                    hud_dirty = true;
                }
            }
        }

        if hud_dirty {
            hud_dirty = false;
            let extra = match &mode {
                Mode::Combat { enemy, .. } => {
                    let e = &room.enemies[*enemy];
                    format!("   {} PV {}", e.kind.name(), e.hp.max(0))
                }
                Mode::Explore => format!("   Ennemis {}", room.enemies.len()),
            };
            hud = Text::new(&format!("Salle {depth}   PV {}/{HERO_MAX_HP}{extra}", hero_hp.max(0)), 4, 0);
        }

        let mut frame = gfx.frame();
        hud.show(&mut frame);
        match &mode {
            Mode::Explore => {
                hero.show(&mut frame);
                for e in &room.enemies {
                    let mut o = Object::new(e.kind.tag().animation_sprite(frame_count / 8));
                    o.set_pos((e.x, e.y));
                    o.show(&mut frame);
                }
                background.show(&mut frame);
            }
            Mode::Combat { enemy, phase } => {
                let e = &room.enemies[*enemy];
                let mut h = Object::new(sprites::ADAM_IDLE.animation_sprite(frame_count / 8));
                h.set_pos((60, 60));
                h.show(&mut frame);
                let ex = match phase {
                    Phase::EnemyAttack { t, .. } => 160 - (100 * *t / ATTACK_LEN).min(84),
                    _ => 160,
                };
                let mut o = Object::new(e.kind.tag().animation_sprite(frame_count / 8));
                o.set_pos((ex, 60));
                o.set_hflip(true);
                o.show(&mut frame);
                if let Phase::Aim { cursor } = phase {
                    let mut c = Text::new("v", 0, 0);
                    for obj in c.objects.iter_mut() {
                        obj.set_pos((BAR_X + *cursor - 2, 90));
                    }
                    c.show(&mut frame);
                    bar_text.show(&mut frame);
                    let _ = &cursor_text;
                }
                if let Some(t) = &combat_text {
                    t.show(&mut frame);
                }
            }
        }
        frame.commit();
    }
}

fn continue_frame(
    gfx: &mut agb::display::Graphics,
    background: &RegularBackground,
    hero: &Object,
    room: &Room,
    hud: &Text,
    _frame_count: usize,
) {
    let _ = room;
    let mut frame = gfx.frame();
    hud.show(&mut frame);
    hero.show(&mut frame);
    background.show(&mut frame);
    frame.commit();
}
