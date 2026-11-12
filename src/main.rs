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
        tiled::{DynamicTile16, RegularBackground, RegularBackgroundSize, TileEffect, TileFormat},
    },
    fixnum::vec2,
    include_aseprite, include_background_gfx, include_font,
    input::{Button, ButtonController},
};

use wfc::{Map, Rng, Terrain};

include_background_gfx!(mod bg, tiles => "gfx/tiles.png");
// Separate palettes: loaded only while the map is hidden (combat, upgrade menu).
include_background_gfx!(mod combatbg, combat => deduplicate "gfx/combat_bg.png");
include_aseprite!(
    mod sprites,
    "gfx/adam_down.gif",
    "gfx/adam_up.gif",
    "gfx/adam_left.gif",
    "gfx/adam_right.gif",
    "gfx/adam_idle.gif",
    "gfx/bob_idle.gif",
    "gfx/alex_idle.gif",
    "gfx/icon_heart.png",
    "gfx/icon_cafe.png",
    "gfx/icon_focus.png",
    "gfx/icon_reflexes.png",
    "gfx/icon_costard.png",
    "gfx/icon_dej.png",
    "gfx/icon_genie.png",
    "gfx/icon_counter.png",
    "gfx/icon_afterwork.png"
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

#[derive(Clone, Copy)]
enum RoomKind {
    OpenSpace,
    Cuisine,
    Archives,
}

impl RoomKind {
    fn random(rng: &mut Rng) -> Self {
        match rng.range(3) {
            0 => RoomKind::OpenSpace,
            1 => RoomKind::Cuisine,
            _ => RoomKind::Archives,
        }
    }
    fn name(self) -> &'static str {
        match self {
            RoomKind::OpenSpace => "Open space",
            RoomKind::Cuisine => "Cuisine",
            RoomKind::Archives => "Archives",
        }
    }
    /// WFC weights: floor, carpet, tile, wall, box, stool.
    fn weights(self) -> [u32; 6] {
        match self {
            RoomKind::OpenSpace => [12, 9, 1, 1, 1, 5],
            RoomKind::Cuisine => [12, 1, 9, 1, 4, 2],
            RoomKind::Archives => [10, 1, 2, 4, 8, 1],
        }
    }
    /// Index in gfx/tiles.png of this room's floor.
    fn floor_tile(self) -> usize {
        match self {
            RoomKind::OpenSpace => 0,
            RoomKind::Cuisine => 8,
            RoomKind::Archives => 9,
        }
    }
}

const TILE_DOOR_CLOSED: usize = 6;
const TILE_DOOR_OPEN: usize = 7;
const TILES_PER_ROW: usize = 20; // 8x8 tiles per row in gfx/tiles.png

struct Room {
    kind: RoomKind,
    map: Map,
    enemies: Vec<Enemy>,
    hero: (i32, i32),
    /// Column of the door in the top wall.
    door_x: usize,
}

impl Room {
    fn door_open(&self) -> bool {
        self.enemies.is_empty()
    }
}

fn new_room(rng: &mut Rng, depth: i32) -> Room {
    let kind = RoomKind::random(rng);
    let mut map = wfc::generate(rng, &kind.weights());
    let walkable: Vec<(usize, usize)> = (0..wfc::H)
        .flat_map(|y| (0..wfc::W).map(move |x| (x, y)))
        .filter(|&(x, y)| !map[y][x].blocks())
        .collect();
    let start = if walkable.is_empty() {
        map[wfc::H / 2][wfc::W / 2] = Terrain::Floor;
        (wfc::W / 2, wfc::H / 2)
    } else {
        walkable[rng.range(walkable.len() as u32) as usize]
    };
    // Guarantee a path from the hero up to row 1, where the door is.
    if (1..=start.1).any(|y| map[y][start.0].blocks()) {
        for y in 1..=start.1 {
            map[y][start.0] = Terrain::Floor;
        }
    }
    let mut reachable = wfc::make_connected(&mut map, start);

    let below_door: Vec<usize> = reachable
        .iter()
        .filter(|&&(x, y)| y == 1 && x > 0 && x < wfc::W - 1)
        .map(|&(x, _)| x)
        .collect();
    let door_x = if below_door.is_empty() {
        start.0
    } else {
        below_door[rng.range(below_door.len() as u32) as usize]
    };

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
    let count = 2 + (depth.min(4) as u32 + 1) / 2;
    for i in 0..count {
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
        kind,
        map,
        enemies,
        hero: (start.0 as i32 * 16, start.1 as i32 * 16 - 16),
        door_x,
    }
}

fn draw_map(background: &mut RegularBackground, room: &Room) {
    for y in 0..wfc::H {
        for x in 0..wfc::W {
            let t = if y == 0 && x == room.door_x {
                if room.door_open() { TILE_DOOR_OPEN } else { TILE_DOOR_CLOSED }
            } else if room.map[y][x] == Terrain::Floor {
                room.kind.floor_tile()
            } else {
                room.map[y][x] as usize
            };
            for dy in 0..2 {
                for dx in 0..2 {
                    let idx = dy * TILES_PER_ROW + t * 2 + dx;
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

/// Combat backdrop: the office illustration (gfx/combat_bg.png, 30x20 tiles).
fn draw_combat_bg(arena: &mut RegularBackground) {
    for y in 0..20i32 {
        for x in 0..30i32 {
            arena.set_tile((x, y), &combatbg::combat.tiles, combatbg::combat.tile_settings[(y * 30 + x) as usize]);
        }
    }
}

/// Camera top-left: centred on the hero, clamped to the room.
fn camera(room: &Room) -> (i32, i32) {
    let max_x = wfc::W as i32 * 16 - agb::display::WIDTH;
    let max_y = wfc::H as i32 * 16 - agb::display::HEIGHT;
    (
        (room.hero.0 + 8 - agb::display::WIDTH / 2).clamp(0, max_x),
        (room.hero.1 + 16 - agb::display::HEIGHT / 2).clamp(0, max_y),
    )
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

const BAR_X: i32 = 72; // multiple of 8: the bar is drawn with 8x8 tiles
const BAR_Y: i32 = 104;
const BAR_W: i32 = 104;
const SWEET_START: i32 = 44;
const SWEET_END: i32 = 60;
const CURSOR_SPEED: i32 = 2;
const ATTACK_LEN: i32 = 80;
/// The "!" signal shows during this window: press A to dodge.
const DODGE_FROM: i32 = 56;
const DODGE_TO: i32 = 72;
const BAR_PALETTE: u8 = 15;
/// Fighters stand on the desks' level of the combat backdrop.
const FIGHTER_Y: i32 = 82;
const AIM_HELP: &str = "Appuie sur A quand le curseur blanc\nest dans la zone VERTE : degats x2 !";
const DODGE_HELP: &str = "Il fonce sur toi ! Appuie sur A\nquand \"!!\" apparait pour esquiver.";

/// Draw the timing bar (grey track, green sweet spot, white cursor) on its own background.
fn draw_bar(bar: &mut RegularBackground, cursor: i32, sweet: &core::ops::RangeInclusive<i32>) {
    for tx in 0..BAR_W / 8 {
        let mut tile = DynamicTile16::new();
        for (row, line) in tile.data_mut().iter_mut().enumerate() {
            let mut v = 0u32;
            for px in 0..8 {
                let x = tx * 8 + px;
                let c = if row == 0 || row == 7 {
                    1 // dark border
                } else if (cursor - 1..=cursor + 1).contains(&x) {
                    4 // cursor
                } else if sweet.contains(&x) {
                    3 // green sweet spot
                } else {
                    2 // grey track
                };
                v |= c << (px * 4);
            }
            *line = v;
        }
        bar.set_tile_dynamic16(
            ((BAR_X / 8) + tx, BAR_Y / 8),
            &tile,
            TileEffect::new(false, false, BAR_PALETTE),
        );
    }
}

// --- Upgrades (roguelite) -------------------------------------------------

/// Run-wide hero stats, upgraded after each cleared room and reset on death.
struct Stats {
    max_hp: i32,
    atk: i32,
    /// Extra pixels on each side of the green zone.
    sweet_bonus: i32,
    /// Extra frames on each side of the dodge window.
    dodge_bonus: i32,
    crit_mult: i32,
    counter: i32,
    lifesteal: i32,
}

impl Stats {
    fn new() -> Self {
        Stats { max_hp: HERO_MAX_HP, atk: 0, sweet_bonus: 0, dodge_bonus: 0, crit_mult: 2, counter: 0, lifesteal: 0 }
    }
    fn sweet(&self) -> core::ops::RangeInclusive<i32> {
        (SWEET_START - self.sweet_bonus).max(0)..=(SWEET_END + self.sweet_bonus).min(BAR_W - 1)
    }
    fn dodge(&self) -> core::ops::RangeInclusive<i32> {
        (DODGE_FROM - self.dodge_bonus)..=(DODGE_TO + self.dodge_bonus).min(ATTACK_LEN - 1)
    }
}

const UPGRADES: [(&str, &str); 8] = [
    ("Cafe serre", "+1 attaque"),
    ("Concentration", "zone verte plus large"),
    ("Reflexes", "esquive plus facile"),
    ("Costard renforce", "+5 PV max"),
    ("Pause dej'", "soin complet"),
    ("Coup de genie", "critique x3 (+1)"),
    ("Contre-attaque", "esquive = 3 degats"),
    ("Afterwork", "+1 PV par coup porte"),
];

static UPGRADE_ICONS: [&Tag; 8] = [
    &sprites::ICON_CAFE,
    &sprites::ICON_FOCUS,
    &sprites::ICON_REFLEXES,
    &sprites::ICON_COSTARD,
    &sprites::ICON_DEJ,
    &sprites::ICON_GENIE,
    &sprites::ICON_COUNTER,
    &sprites::ICON_AFTERWORK,
];

const MENU_Y0: i32 = 48;
const MENU_STEP: i32 = 30;

fn apply_upgrade(i: usize, stats: &mut Stats, hp: &mut i32) {
    match i {
        0 => stats.atk += 1,
        1 => stats.sweet_bonus += 5,
        2 => stats.dodge_bonus += 3,
        3 => {
            stats.max_hp += 5;
            *hp += 5;
        }
        4 => *hp = stats.max_hp,
        5 => stats.crit_mult += 1,
        6 => stats.counter += 3,
        _ => stats.lifesteal += 1,
    }
}

fn pick_upgrades(rng: &mut Rng) -> [usize; 3] {
    let mut pool: Vec<usize> = (0..UPGRADES.len()).collect();
    let mut out = [0; 3];
    for o in out.iter_mut() {
        *o = pool.swap_remove(rng.range(pool.len() as u32) as usize);
    }
    out
}

fn upgrade_menu(options: &[usize; 3], sel: usize) -> Text {
    let mut menu = Text::new("SALLE TERMINEE ! Choisis une competence :", 16, 22);
    for (i, &u) in options.iter().enumerate() {
        let (name, desc) = UPGRADES[u];
        let y = MENU_Y0 + i as i32 * MENU_STEP;
        let marker = if i == sel { ">" } else { "" };
        menu.objects.extend(Text::new(marker, 14, y).objects);
        menu.objects.extend(Text::new(&format!("{name}\n{desc}"), 50, y - 4).objects);
    }
    menu.objects.extend(Text::new("Haut/Bas pour choisir, A pour valider", 16, 140).objects);
    menu
}

/// Dark rounded-ish panel (generated, no asset): fills tile rect [x0,x1]x[y0,y1].
fn draw_panel(panel: &mut RegularBackground, x0: i32, y0: i32, x1: i32, y1: i32) {
    // 9-slice: border colour 2, fill colour 1 (bar palette).
    let make = |top: bool, bottom: bool, left: bool, right: bool| {
        let mut tile = DynamicTile16::new();
        for (row, line) in tile.data_mut().iter_mut().enumerate() {
            let mut v = 0u32;
            for px in 0..8u32 {
                let edge = (top && row == 0) || (bottom && row == 7) || (left && px == 0) || (right && px == 7);
                v |= if edge { 2 } else { 1 } << (px * 4);
            }
            *line = v;
        }
        tile
    };
    let mut tiles: Vec<DynamicTile16> = Vec::new();
    for i in 0..9 {
        let (r, c) = (i / 3, i % 3);
        tiles.push(make(r == 0, r == 2, c == 0, c == 2));
    }
    for y in y0..=y1 {
        for x in x0..=x1 {
            let r = if y == y0 { 0 } else if y == y1 { 2 } else { 1 };
            let c = if x == x0 { 0 } else if x == x1 { 2 } else { 1 };
            panel.set_tile_dynamic16((x, y), &tiles[r * 3 + c], TileEffect::new(false, false, BAR_PALETTE));
        }
    }
}

/// Blank the whole UI layer (reused, never reallocated: agb only has 8 screenblocks
/// and the scrolling map already takes 4).
fn clear_bar(ui: &mut RegularBackground) {
    let empty = DynamicTile16::new();
    for tx in 0..BAR_W / 8 {
        ui.set_tile_dynamic16(((BAR_X / 8) + tx, BAR_Y / 8), &empty, TileEffect::new(false, false, BAR_PALETTE));
    }
}

fn clear_ui(ui: &mut RegularBackground) {
    let empty = DynamicTile16::new(); // all pixels transparent
    for y in 0..20 {
        for x in 0..30 {
            ui.set_tile_dynamic16((x, y), &empty, TileEffect::new(false, false, BAR_PALETTE));
        }
    }
}

// --- Game -----------------------------------------------------------------

enum Mode {
    Explore,
    Combat { enemy: usize, phase: Phase },
    Upgrade { options: [usize; 3], sel: usize },
}

#[agb::entry]
fn main(mut gba: agb::Gba) -> ! {
    let mut gfx = gba.graphics.get();
    let mut input = ButtonController::new();
    let set_palettes = |gfx: &mut agb::display::Graphics, combat: bool| {
        gfx.set_background_palettes(if combat { combatbg::PALETTES } else { bg::PALETTES });
        for (i, c) in [0x0000u16, 0x1084, 0x4210, 0x03E0, 0x7FFF].into_iter().enumerate() {
            gfx.set_background_palette_colour(BAR_PALETTE as usize, i, Rgb15::new(c));
        }
    };
    set_palettes(&mut gfx, false);
    let mut combat_palettes = false;

    // 64x64 tiles = 512x512 px: enough for a 30x20 room of 16x16 cells.
    let mut background = RegularBackground::new(
        Priority::P3,
        RegularBackgroundSize::Background64x64,
        TileFormat::FourBpp,
    );
    // Combat backdrop.
    let mut arena = RegularBackground::new(
        Priority::P3,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );
    draw_combat_bg(&mut arena);
    let mut bar_visible = false;
    // UI layer: panels + timing bar share one background.
    let mut ui = RegularBackground::new(
        Priority::P1,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );

    // Title screen: seed comes from how long the player waits.
    let title = Text::new(
        "THEODO RPG\n\nBouge avec les fleches. Touche un collegue pour combattre.\n\nTON TOUR : appuie sur A quand le curseur blanc est dans la zone VERTE (degats x2).\n\nSON TOUR : appuie sur A quand le \"!\" apparait pour esquiver.\n\nSTART pour jouer",
        8,
        8,
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
    let mut stats = Stats::new();
    let mut hero_hp = stats.max_hp;
    let mut menu = Text::new("", 0, 0);
    let mut room = new_room(&mut rng, depth);
    draw_map(&mut background, &room);

    let mut hero = Object::new(sprites::ADAM_DOWN.sprite(0));
    let mut facing: &'static Tag = &sprites::ADAM_DOWN;
    let mut mode = Mode::Explore;
    let mut frame_count: usize = 0;
    let mut hud = Text::new(&format!("Salle {depth}   PV {hero_hp}/{HERO_MAX_HP}"), 4, 0);
    let mut hud_dirty = true;
    let mut combat_text: Option<Text> = None;
    let alert = Text::new("!!", 62, 40);
    let mut heart = Object::new(sprites::ICON_HEART.sprite(0));
    heart.set_pos((2, -2));
    let mut title_text = Text::new("", 0, 0);

    loop {
        frame_count = frame_count.wrapping_add(1);
        input.update();

        match &mut mode {
            Mode::Upgrade { .. } => {} // handled below
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
                // Walk into the open door (top wall) to reach the next room.
                let feet = ((room.hero.0 + 8) / 16, (room.hero.1 + 28) / 16);
                if room.door_open()
                    && feet == (room.door_x as i32, 1)
                    && room.hero.1 + 26 <= 16 + 3
                {
                    depth += 1;
                    hero_hp = (hero_hp + 3).min(stats.max_hp);
                    room = new_room(&mut rng, depth);
                    draw_map(&mut background, &room);
                    hud_dirty = true;
                }
                let cam = camera(&room);
                hero.set_pos((room.hero.0 - cam.0, room.hero.1 - cam.1));

                if let Some(i) = room.enemies.iter().position(|e| {
                    (e.x - room.hero.0).abs() < 14 && (e.y - room.hero.1).abs() < 14
                }) {
                    let e = &room.enemies[i];
                    let _ = e;
                    clear_ui(&mut ui);
                    draw_panel(&mut ui, 0, 15, 29, 19);
                    title_text = Text::new("TON TOUR", 96, 20);
                    combat_text = Some(Text::new(AIM_HELP, 8, 126));
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
                        *cursor += CURSOR_SPEED;
                        let pressed = input.is_just_pressed(Button::A);
                        if pressed || *cursor >= BAR_W {
                            let base = 4 + stats.atk;
                            let (dmg, msg) = if pressed && stats.sweet().contains(cursor) {
                                let d = base * stats.crit_mult;
                                (d, format!("PARFAIT ! Dans le vert : {d} degats"))
                            } else if pressed {
                                (base, format!("A cote du vert : {base} degats"))
                            } else {
                                (base / 2, format!("Pas appuye a temps : {} degats", base / 2))
                            };
                            e.hp -= dmg;
                            hero_hp = (hero_hp + stats.lifesteal).min(stats.max_hp);
                            next = Some(Phase::Message {
                                text: msg,
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
                            let dodged = matches!(pressed, Some(p) if stats.dodge().contains(p));
                            let dmg = if dodged { 0 } else { 2 + depth };
                            hero_hp -= dmg;
                            if dodged {
                                e.hp -= stats.counter;
                            }
                            next = Some(Phase::Message {
                                text: if dodged && stats.counter > 0 {
                                    format!("ESQUIVE + contre-attaque : {} degats", stats.counter)
                                } else if dodged {
                                    "ESQUIVE ! Aucun degat".into()
                                } else if pressed.is_some() {
                                    format!("Mauvais timing ! -{dmg} PV")
                                } else {
                                    format!("Pas d'esquive ! -{dmg} PV")
                                },
                                timer: 50,
                                then: if hero_hp <= 0 {
                                    Then::Defeat
                                } else if e.hp <= 0 {
                                    Then::Victory
                                } else {
                                    Then::PlayerTurn
                                },
                            });
                        }
                    }
                    Phase::Message { timer, then, .. } => {
                        *timer -= 1;
                        if *timer <= 0 {
                            match *then {
                                Then::EnemyTurn => {
                                    title_text = Text::new("SON TOUR", 96, 20);
                                    combat_text = Some(Text::new(DODGE_HELP, 8, 126));
                                    *phase = Phase::EnemyAttack { t: 0, pressed: None };
                                    hud_dirty = true;
                                }
                                Then::PlayerTurn => {
                                    title_text = Text::new("TON TOUR", 96, 20);
                                    combat_text = Some(Text::new(AIM_HELP, 8, 126));
                                    *phase = Phase::Aim { cursor: 0 };
                                    hud_dirty = true;
                                }
                                Then::Victory => {
                                    room.enemies.swap_remove(*enemy);
                                    combat_text = None;
                                    if room.door_open() {
                                        // Last enemy down: the door opens, pick an upgrade.
                                        draw_map(&mut background, &room);
                                        let options = pick_upgrades(&mut rng);
                                        menu = upgrade_menu(&options, 0);
                                        clear_ui(&mut ui);
                                        draw_panel(&mut ui, 1, 2, 28, 19);
                                        mode = Mode::Upgrade { options, sel: 0 };
                                    } else {
                                        mode = Mode::Explore;
                                    }
                                    hud_dirty = true;
                                    // Step away so we don't re-trigger instantly.
                                    continue_frame(&mut gfx, &background, &hero, &room, &hud, frame_count);
                                    continue;
                                }
                                Then::Defeat => {
                                    depth = 1;
                                    stats = Stats::new();
                                    hero_hp = stats.max_hp;
                                    room = new_room(&mut rng, depth);
                                    draw_map(&mut background, &room);
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
                        combat_text = Some(Text::new(text, 8, 126));
                    }
                    *phase = p;
                    hud_dirty = true;
                }
            }
        }

        if let Mode::Upgrade { options, sel } = &mut mode {
            let old = *sel;
            if input.is_just_pressed(Button::Up) {
                *sel = (*sel + 2) % 3;
            }
            if input.is_just_pressed(Button::Down) {
                *sel = (*sel + 1) % 3;
            }
            if *sel != old {
                menu = upgrade_menu(options, *sel);
            }
            if input.is_just_pressed(Button::A) {
                apply_upgrade(options[*sel], &mut stats, &mut hero_hp);
                mode = Mode::Explore;
                hud_dirty = true;
            }
        }

        if hud_dirty {
            hud_dirty = false;
            let extra = match &mode {
                Mode::Combat { enemy, .. } => {
                    let e = &room.enemies[*enemy];
                    format!("   {} PV {}", e.kind.name(), e.hp.max(0))
                }
                Mode::Upgrade { .. } => String::new(),
                Mode::Explore if room.door_open() => "   Porte ouverte ! (en haut)".into(),
                Mode::Explore => format!("   Ennemis {}", room.enemies.len()),
            };
            hud = Text::new(
                &format!("{}/{}   {} {depth}{extra}", hero_hp.max(0), stats.max_hp, room.kind.name()),
                20,
                0,
            );
        }

        let want_combat = !matches!(mode, Mode::Explore);
        if want_combat != combat_palettes {
            combat_palettes = want_combat;
            set_palettes(&mut gfx, want_combat);
        }

        let mut frame = gfx.frame();
        hud.show(&mut frame);
        heart.show(&mut frame);
        match &mode {
            Mode::Explore => {
                hero.show(&mut frame);
                let cam = camera(&room);
                for e in &room.enemies {
                    let (sx, sy) = (e.x - cam.0, e.y - cam.1);
                    if !(-16..agb::display::WIDTH).contains(&sx) || !(-32..agb::display::HEIGHT).contains(&sy) {
                        continue;
                    }
                    let mut o = Object::new(e.kind.tag().animation_sprite(frame_count / 8));
                    o.set_pos((sx, sy));
                    o.show(&mut frame);
                }
                background.set_scroll_pos(cam);
                background.show(&mut frame);
            }
            Mode::Upgrade { options, .. } => {
                arena.show(&mut frame);
                ui.show(&mut frame);
                menu.show(&mut frame);
                for (i, &u) in options.iter().enumerate() {
                    let mut o = Object::new(UPGRADE_ICONS[u].sprite(0));
                    o.set_pos((28, MENU_Y0 - 2 + i as i32 * MENU_STEP));
                    o.show(&mut frame);
                }
            }
            Mode::Combat { enemy, phase } => {
                arena.show(&mut frame);
                ui.show(&mut frame);
                let e = &room.enemies[*enemy];
                let mut h = Object::new(sprites::ADAM_IDLE.animation_sprite(frame_count / 8));
                h.set_pos((60, FIGHTER_Y));
                h.show(&mut frame);
                // The enemy charges and reaches the hero in the middle of the dodge window.
                let impact = (DODGE_FROM + DODGE_TO) / 2;
                let ex = match phase {
                    Phase::EnemyAttack { t, .. } => 160 - 84 * (*t).min(impact) / impact,
                    _ => 160,
                };
                let mut o = Object::new(e.kind.tag().animation_sprite(frame_count / 8));
                o.set_pos((ex, FIGHTER_Y));
                o.set_hflip(true);
                o.show(&mut frame);
                if !matches!(phase, Phase::Message { .. }) {
                    title_text.show(&mut frame);
                }
                match phase {
                    Phase::Aim { cursor } => {
                        draw_bar(&mut ui, *cursor, &stats.sweet());
                        bar_visible = true;
                    }
                    _ if !matches!(phase, Phase::Aim { .. }) && bar_visible => {
                        clear_bar(&mut ui);
                        bar_visible = false;
                    }
                    Phase::EnemyAttack { t, .. } if stats.dodge().contains(t) => {
                        alert.show(&mut frame);
                    }
                    _ => {}
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
