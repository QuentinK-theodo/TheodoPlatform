//! Theodo RPG — a tiny Mario & Luigi–style roguelite demo for the GBA.
#![no_std]
#![no_main]
#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, reexport_test_harness_main = "test_main")]
#![cfg_attr(test, test_runner(agb::test_runner::test_runner))]

extern crate alloc;

mod dungeon;
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

use dungeon::{BuiltRoom, Floor};
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
    "gfx/icon_afterwork.png",
    "gfx/shadow.png",
    "gfx/mm_room.png",
    "gfx/mm_current.png",
    "gfx/mm_exit.png",
    "gfx/mm_unknown.png"
);

static FONT: Font = include_font!("fonts/ark-pixel-10px-proportional-ja.ttf", 10);

static TEXT_PALETTE: &Palette16 = const {
    let mut palette = [Rgb15::BLACK; 16];
    palette[1] = Rgb15::WHITE;
    &Palette16::new(palette)
};

const HERO_MAX_HP: i32 = 20;
/// Intro card duration (~3 s), skippable with A.
const INTRO_FRAMES: i32 = 180;

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
    /// Frees the sprite VRAM held by the previous text when assigned over it.
    fn empty() -> Self {
        Text { objects: Vec::new() }
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
}

struct Enemy {
    kind: Kind,
    x: i32,
    y: i32,
    hp: i32,
}

const TILE_DOOR_CLOSED: usize = 6;
const TILE_DOOR_OPEN: usize = 7;
const TILES_PER_ROW: usize = 20; // 8x8 tiles per row in gfx/tiles.png

/// The room the hero is currently in.
struct Room {
    idx: usize,
    built: BuiltRoom,
    enemies: Vec<Enemy>,
    hero: (i32, i32),
}

impl Room {
    fn door_open(&self) -> bool {
        self.enemies.is_empty()
    }
    fn map(&self) -> &Map {
        &self.built.map
    }
    fn template(&self, floor: &Floor) -> &'static dungeon::Template {
        &dungeon::TEMPLATES[floor.rooms[self.idx].template]
    }
    fn door_at(&self, cx: i32, cy: i32) -> Option<usize> {
        (0..4).find(|&d| self.built.doors[d] == Some((cx as usize, cy as usize)))
    }
}

fn cell_to_pos((x, y): (usize, usize)) -> (i32, i32) {
    (x as i32 * 16, y as i32 * 16 - 16)
}

/// Enter room `idx`. `came_from` is the direction we travelled (so we appear
/// at the opposite door); `None` puts the hero in the centre.
fn enter_room(floor: &mut Floor, idx: usize, came_from: Option<usize>, level: i32, rng: &mut Rng) -> Room {
    let built = dungeon::build_room(floor, idx);
    let slot = &mut floor.rooms[idx];
    slot.visited = true;
    let start = match came_from {
        Some(d) => built.inside((d + 2) % 4),
        None => (built.map.w / 2, built.map.h / 2),
    };

    let mut enemies = Vec::new();
    if !slot.cleared {
        let boss = idx == floor.exit;
        let mut spots: Vec<(usize, usize)> = built
            .spawns
            .iter()
            .copied()
            .filter(|&(x, y)| x.abs_diff(start.0) + y.abs_diff(start.1) >= 4)
            .collect();
        let wanted = if boss { 4 } else { (2 + level as usize / 2).min(4) };
        while enemies.len() < wanted && !spots.is_empty() {
            let (x, y) = spots.swap_remove(rng.range(spots.len() as u32) as usize);
            let (ex, ey) = cell_to_pos((x, y));
            enemies.push(Enemy {
                kind: if rng.range(2) == 0 { Kind::Bob } else { Kind::Alex },
                x: ex,
                y: ey,
                hp: 6 + level * 2 + rng.range(5) as i32 + if boss { 4 } else { 0 },
            });
        }
        if enemies.is_empty() {
            slot.cleared = true;
        }
    }

    Room { idx, built, enemies, hero: cell_to_pos(start) }
}

fn draw_map(background: &mut RegularBackground, room: &Room, floor: &Floor) {
    let map = room.map();
    let floor_tile = room.template(floor).floor_tile;
    for y in 0..map.h {
        for x in 0..map.w {
            let t = if room.door_at(x as i32, y as i32).is_some() {
                if room.door_open() { TILE_DOOR_OPEN } else { TILE_DOOR_CLOSED }
            } else if map.at(x, y) == Terrain::Floor {
                floor_tile
            } else {
                map.at(x, y) as usize
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
    // Pin every map tile in VRAM by also placing them off-screen (rows 22-23):
    // room changes then never free/reallocate map tiles.
    for i in 0..TILES_PER_ROW * 2 {
        let (x, y) = ((i % TILES_PER_ROW) as i32, 22 + (i / TILES_PER_ROW) as i32);
        arena.set_tile((x, y), &bg::tiles.tiles, bg::tiles.tile_settings[i]);
    }
}

/// Shadow under a combat fighter's feet (shown after the fighter so it sits behind).
fn show_shadow(frame: &mut GraphicsFrame, x: i32) {
    show_shadow_at(frame, x, FIGHTER_Y + 26);
}

fn show_shadow_at(frame: &mut GraphicsFrame, x: i32, y: i32) {
    let mut s = Object::new(sprites::SHADOW.sprite(0));
    s.set_pos((x, y));
    s.show(frame);
}

/// Camera top-left: centred on the hero, clamped to the room.
fn camera(room: &Room) -> (i32, i32) {
    let max_x = (room.map().w as i32 * 16 - agb::display::WIDTH).max(0);
    let max_y = (room.map().h as i32 * 16 - agb::display::HEIGHT).max(0);
    (
        (room.hero.0 + 8 - agb::display::WIDTH / 2).clamp(0, max_x),
        (room.hero.1 + 16 - agb::display::HEIGHT / 2).clamp(0, max_y),
    )
}

/// Feet cell of the hero sprite at (px, py).
fn feet_cell(px: i32, py: i32) -> (i32, i32) {
    ((px + 8) / 16, (py + 28) / 16)
}

fn blocked(room: &Room, px: i32, py: i32) -> bool {
    let map = room.map();
    // Feet hitbox: 10x6 at the bottom of the 16x32 sprite.
    for (fx, fy) in [(3, 26), (12, 26), (3, 31), (12, 31)] {
        let (cx, cy) = ((px + fx).div_euclid(16), (py + fy).div_euclid(16));
        if cx < 0 || cy < 0 || cx >= map.w as i32 || cy >= map.h as i32 {
            return true;
        }
        if room.door_at(cx, cy).is_some() {
            if !room.door_open() {
                return true;
            }
        } else if map.at(cx as usize, cy as usize).blocks() {
            return true;
        }
    }
    false
}

/// Floor size grows a little with depth.
fn rooms_for_level(level: i32) -> usize {
    (6 + level as usize).min(9)
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
const MENU_STEP: i32 = 32; // multiple of 8 so the highlight box aligns with tiles

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

// --- UI tiles -------------------------------------------------------------
//
// All UI tiles are created once and kept alive for the whole game. Creating and
// dropping dynamic tiles every frame made agb's VRAM manager hand out tiles still
// used by the combat backdrop (visible corruption), so nothing here churns.

fn fill_tile(tile: &mut DynamicTile16, mut colour: impl FnMut(usize, usize) -> u32) {
    for (row, line) in tile.data_mut().iter_mut().enumerate() {
        let mut v = 0u32;
        for px in 0..8 {
            v |= colour(px, row) << (px * 4);
        }
        *line = v;
    }
}

/// 9-slice set: index = row * 3 + col (0 = top/left edge, 2 = bottom/right edge).
fn nine_slice(fill: u32, border: u32) -> Vec<DynamicTile16> {
    (0..9)
        .map(|i| {
            let (r, c) = (i / 3, i % 3);
            let mut t = DynamicTile16::new();
            fill_tile(&mut t, |px, row| {
                let edge = (r == 0 && row == 0) || (r == 2 && row == 7) || (c == 0 && px == 0) || (c == 2 && px == 7);
                if edge { border } else { fill }
            });
            t
        })
        .collect()
}

struct UiTiles {
    empty: DynamicTile16,
    panel: Vec<DynamicTile16>,
    highlight: Vec<DynamicTile16>,
    bar: Vec<DynamicTile16>,
}

impl UiTiles {
    fn new() -> Self {
        let mut empty = DynamicTile16::new();
        fill_tile(&mut empty, |_, _| 0);
        UiTiles {
            empty,
            panel: nine_slice(1, 2),
            highlight: nine_slice(2, 4),
            bar: (0..BAR_W / 8).map(|_| DynamicTile16::new()).collect(),
        }
    }

    fn put(ui: &mut RegularBackground, x: i32, y: i32, tile: &DynamicTile16) {
        ui.set_tile_dynamic16((x, y), tile, TileEffect::new(false, false, BAR_PALETTE));
    }

    fn clear(&self, ui: &mut RegularBackground) {
        for y in 0..20 {
            for x in 0..30 {
                Self::put(ui, x, y, &self.empty);
            }
        }
    }

    fn draw_box(ui: &mut RegularBackground, set: &[DynamicTile16], x0: i32, y0: i32, x1: i32, y1: i32) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                let r = if y == y0 { 0 } else if y == y1 { 2 } else { 1 };
                let c = if x == x0 { 0 } else if x == x1 { 2 } else { 1 };
                Self::put(ui, x, y, &set[r * 3 + c]);
            }
        }
    }

    fn panel(&self, ui: &mut RegularBackground, x0: i32, y0: i32, x1: i32, y1: i32) {
        Self::draw_box(ui, &self.panel, x0, y0, x1, y1);
    }

    /// Menu panel with the selected option highlighted.
    fn upgrade_panel(&self, ui: &mut RegularBackground, sel: usize) {
        self.panel(ui, 1, 2, 28, 19);
        let top = (MENU_Y0 - 6 + sel as i32 * MENU_STEP) / 8;
        Self::draw_box(ui, &self.highlight, 2, top, 27, top + 3);
    }

    /// Timing bar: grey track, green sweet spot, white cursor (pixels rewritten in place).
    fn draw_bar(&mut self, ui: &mut RegularBackground, cursor: i32, sweet: &core::ops::RangeInclusive<i32>) {
        for (tx, tile) in self.bar.iter_mut().enumerate() {
            fill_tile(tile, |px, row| {
                let x = tx as i32 * 8 + px as i32;
                if row == 0 || row == 7 {
                    1
                } else if (cursor - 1..=cursor + 1).contains(&x) {
                    4
                } else if sweet.contains(&x) {
                    3
                } else {
                    2
                }
            });
        }
        for (tx, tile) in self.bar.iter().enumerate() {
            Self::put(ui, BAR_X / 8 + tx as i32, BAR_Y / 8, tile);
        }
    }

    fn clear_bar(&self, ui: &mut RegularBackground) {
        for tx in 0..BAR_W / 8 {
            Self::put(ui, BAR_X / 8 + tx, BAR_Y / 8, &self.empty);
        }
    }
}

// --- Game -----------------------------------------------------------------

/// Combat intro cards: (who you're fighting, the situation).
const INTROS: &[(&str, &str)] = &[
    ("Ton PM", "Ton PM te refile un ticket\nen fin de sprint !\nTrouve vite une excuse !"),
    ("Le tech lead", "Ton tech lead refuse ta PR !\nExplique-lui ta facon de penser !"),
    ("Le KAM", "Le KAM a promis la feature\nau client pour demain matin !\nDefends ton estimation !"),
    ("Le LPM", "Le LPM veut re-chiffrer tout\nle backlog avant midi !\nNegocie un delai !"),
    ("Un partner", "Un partner passe la tete :\n\"Tu as 5 minutes ?\"\nIl en faudra 50."),
    ("Le client", "Le client demande pourquoi le\nbouton n'est pas \"plus bleu\".\nConvaincs-le !"),
    ("Le client du client", "Le client de ton client a teste\nen prod... sur IE6.\nRassure-le !"),
    ("Le PO", "Le PO a change les specs\npendant la daily !\nRecadre le scope !"),
    ("Un dev senior", "Un dev senior relit ton code :\n\"Pourquoi pas en Rust ?\"\nArgumente !"),
    ("Le coach agile", "Le coach agile organise une\nretro de la retro !\nSurvis a l'atelier post-it !"),
    ("L'astreinte", "La prod tombe vendredi a 17h59 !\nTrouve le coupable\n(idealement, pas toi)."),
    ("Le staffing", "Le staffing te propose une\nmission en COBOL de 3 ans !\nDecline poliment !"),
    ("Le KAM", "Le KAM a vendu \"de l'IA\"\nau client. Personne ne sait\nlaquelle. Improvise !"),
    ("Le partner", "Le partner veut une demo\n\"rapide\" devant le COMEX.\nC'est dans 10 minutes !"),
];

enum Mode {
    Explore,
    /// Loading card before a fight ("Ton PM te refile un ticket !").
    Intro { enemy: usize, timer: i32 },
    Combat { enemy: usize, phase: Phase },
    Upgrade { options: [usize; 3], sel: usize },
}

// Texts are reset to `Text::empty()` before being rebuilt on purpose: it frees
// their sprite VRAM first (see `Text::empty`).
#[allow(unused_assignments)]
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
    let mut ui_tiles = UiTiles::new();
    let mut bar_visible = false;
    // UI layer: panels + timing bar share one background.
    let mut ui = RegularBackground::new(
        Priority::P1,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );

    // Title screen: seed comes from how long the player waits.
    let title = Text::new(
        "THEODO RPG\n\nVide les pieces et trouve le bureau du boss (rouge sur la carte). Touche un collegue pour combattre.\n\nTON TOUR : appuie sur A quand le curseur blanc est dans la zone VERTE (degats x2).\n\nSON TOUR : appuie sur A quand le \"!\" apparait pour esquiver.\n\nSTART pour jouer",
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
    drop(title); // big text: give its sprite VRAM back

    let mut rng = Rng::new(seed);
    let mut depth = 1; // floor ("etage") number
    let mut stats = Stats::new();
    let mut hero_hp = stats.max_hp;
    let mut menu = Text::new("", 0, 0);
    let mut floor = dungeon::generate_floor(&mut rng, rooms_for_level(depth));
    let mut room = enter_room(&mut floor, 0, None, depth, &mut rng);
    draw_map(&mut background, &room, &floor);

    let mut hero = Object::new(sprites::ADAM_DOWN.sprite(0));
    let mut facing: &'static Tag = &sprites::ADAM_DOWN;
    let mut mode = Mode::Explore;
    let mut frame_count: usize = 0;
    let mut hud = Text::new(&format!("Salle {depth}   PV {hero_hp}/{HERO_MAX_HP}"), 4, 0);
    let mut hud_dirty = true;
    let mut combat_text: Option<Text> = None;
    let alert = Text::new("!!", 62, 40);
    let mut foe_name: &str = "";
    let mut heart = Object::new(sprites::ICON_HEART.sprite(0));
    heart.set_pos((2, -2));
    let mut title_text = Text::new("", 0, 0);

    loop {
        frame_count = frame_count.wrapping_add(1);
        input.update();

        match &mut mode {
            Mode::Upgrade { .. } => {} // handled below
            Mode::Intro { enemy, timer } => {
                *timer -= 1;
                // Short lock so a held A from exploring doesn't skip the card instantly.
                let skip = *timer < INTRO_FRAMES - 20 && input.is_just_pressed(Button::A);
                if *timer <= 0 || skip {
                    let enemy = *enemy;
                    ui_tiles.clear(&mut ui);
                    ui_tiles.panel(&mut ui, 0, 15, 29, 19);
                    title_text = Text::empty();
                    title_text = Text::new("TON TOUR", 96, 20);
                    combat_text = None;
                    combat_text = Some(Text::new(AIM_HELP, 8, 126));
                    mode = Mode::Combat { enemy, phase: Phase::Aim { cursor: 0 } };
                    hud_dirty = true;
                }
            }
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
                    if !blocked(&room, room.hero.0 + dx * speed, room.hero.1) {
                        room.hero.0 += dx * speed;
                    }
                    if !blocked(&room, room.hero.0, room.hero.1 + dy * speed) {
                        room.hero.1 += dy * speed;
                    }
                    hero.set_sprite(facing.animation_sprite(frame_count / 6));
                } else {
                    hero.set_sprite(facing.sprite(0));
                }
                // Walk into an open door to reach the neighbouring room.
                let (fx, fy) = feet_cell(room.hero.0, room.hero.1);
                if room.door_open() {
                    if let Some(dir) = room.door_at(fx, fy) {
                        if let Some(next) = floor.neighbour(room.idx, dir) {
                            room = enter_room(&mut floor, next, Some(dir), depth, &mut rng);
                            draw_map(&mut background, &room, &floor);
                            hud_dirty = true;
                        }
                    }
                }
                let cam = camera(&room);
                hero.set_pos((room.hero.0 - cam.0, room.hero.1 - cam.1));

                if let Some(i) = room.enemies.iter().position(|e| {
                    (e.x - room.hero.0).abs() < 14 && (e.y - room.hero.1).abs() < 14
                }) {
                    let (who, line) = INTROS[rng.range(INTROS.len() as u32) as usize];
                    foe_name = who;
                    ui_tiles.clear(&mut ui);
                    ui_tiles.panel(&mut ui, 2, 4, 27, 15);
                    combat_text = None;
                    combat_text = Some(Text::new(line, 28, 52));
                    title_text = Text::empty();
                    title_text = Text::new("A pour combattre", 72, 104);
                    mode = Mode::Intro { enemy: i, timer: INTRO_FRAMES };
                    hud_dirty = true;
                }
            }
            Mode::Combat { enemy, phase } => {
                let e = &mut room.enemies[*enemy];
                let mut next: Option<Phase> = None;
                match phase {
                    Phase::Aim { cursor } => {
                        // Judge the cursor where the player saw it (drawn last frame),
                        // counting its 3px width, then move it.
                        let pressed = input.is_just_pressed(Button::A);
                        let sweet = stats.sweet();
                        let hit = (*cursor - 1..=*cursor + 1).any(|x| sweet.contains(&x));
                        if !pressed {
                            *cursor += CURSOR_SPEED;
                        }
                        if pressed || *cursor >= BAR_W {
                            let base = 4 + stats.atk;
                            let (dmg, msg) = if pressed && hit {
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
                            // One frame of tolerance: the "!!" seen was drawn the previous frame.
                            let dodged = matches!(pressed, Some(p) if stats.dodge().contains(p) || stats.dodge().contains(&(*p - 1)));
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
                                    title_text = Text::empty();
                                    title_text = Text::new("SON TOUR", 96, 20);
                                    combat_text = None;
                                    combat_text = Some(Text::new(DODGE_HELP, 8, 126));
                                    *phase = Phase::EnemyAttack { t: 0, pressed: None };
                                    hud_dirty = true;
                                }
                                Then::PlayerTurn => {
                                    title_text = Text::empty();
                                    title_text = Text::new("TON TOUR", 96, 20);
                                    combat_text = None;
                                    combat_text = Some(Text::new(AIM_HELP, 8, 126));
                                    *phase = Phase::Aim { cursor: 0 };
                                    hud_dirty = true;
                                }
                                Then::Victory => {
                                    room.enemies.swap_remove(*enemy);
                                    combat_text = None;
                                    title_text = Text::empty();
                                    if room.door_open() {
                                        // Last enemy down: doors open, pick an upgrade.
                                        floor.rooms[room.idx].cleared = true;
                                        draw_map(&mut background, &room, &floor);
                                        let options = pick_upgrades(&mut rng);
                                        menu = Text::empty();
                                        menu = upgrade_menu(&options, 0);
                                        ui_tiles.clear(&mut ui);
                                        ui_tiles.upgrade_panel(&mut ui, 0);
                                        mode = Mode::Upgrade { options, sel: 0 };
                                    } else {
                                        mode = Mode::Explore;
                                    }
                                    hud_dirty = true;
                                    continue;
                                }
                                Then::Defeat => {
                                    depth = 1;
                                    stats = Stats::new();
                                    hero_hp = stats.max_hp;
                                    floor = dungeon::generate_floor(&mut rng, rooms_for_level(depth));
                                    room = enter_room(&mut floor, 0, None, depth, &mut rng);
                                    draw_map(&mut background, &room, &floor);
                                    combat_text = None;
                                    mode = Mode::Explore;
                                    hud_dirty = true;
                                    continue;
                                }
                            }
                        }
                    }
                }
                if let Some(p) = next {
                    if let Phase::Message { text, .. } = &p {
                        combat_text = None;
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
                menu = Text::empty();
                menu = upgrade_menu(options, *sel);
                ui_tiles.upgrade_panel(&mut ui, *sel);
            }
            if input.is_just_pressed(Button::A) {
                apply_upgrade(options[*sel], &mut stats, &mut hero_hp);
                menu = Text::empty();
                mode = Mode::Explore;
                hud_dirty = true;
                if room.idx == floor.exit {
                    // Boss office cleared: next floor.
                    depth += 1;
                    hero_hp = (hero_hp + 3).min(stats.max_hp);
                    floor = dungeon::generate_floor(&mut rng, rooms_for_level(depth));
                    room = enter_room(&mut floor, 0, None, depth, &mut rng);
                    draw_map(&mut background, &room, &floor);
                }
            }
        }

        if hud_dirty {
            hud_dirty = false;
            let extra = match &mode {
                Mode::Combat { enemy, .. } => {
                    let e = &room.enemies[*enemy];
                    format!("   {} PV {}", foe_name, e.hp.max(0))
                }
                Mode::Upgrade { .. } | Mode::Intro { .. } => String::new(),
                Mode::Explore if room.idx == floor.exit && !room.door_open() => "  BOSS".into(),
                Mode::Explore => String::new(),
            };
            hud = Text::empty();
            hud = Text::new(
                &format!("{}/{}  Etage {depth}  {}{extra}", hero_hp.max(0), stats.max_hp, room.template(&floor).name),
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
                show_minimap(&mut frame, &floor, room.idx);
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
            Mode::Intro { enemy, .. } => {
                arena.show(&mut frame);
                ui.show(&mut frame);
                if let Some(t) = &combat_text {
                    t.show(&mut frame);
                }
                if frame_count / 30 % 2 == 0 {
                    title_text.show(&mut frame); // blinking "A pour combattre"
                }
                let mut o = Object::new(room.enemies[*enemy].kind.tag().animation_sprite(frame_count / 8));
                o.set_pos((112, 128));
                o.show(&mut frame);
                show_shadow_at(&mut frame, 112, 154);
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
                show_shadow(&mut frame, 60);
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
                show_shadow(&mut frame, ex);
                if !matches!(phase, Phase::Message { .. }) {
                    title_text.show(&mut frame);
                }
                match phase {
                    Phase::Aim { cursor } => {
                        ui_tiles.draw_bar(&mut ui, *cursor, &stats.sweet());
                        bar_visible = true;
                    }
                    _ if !matches!(phase, Phase::Aim { .. }) && bar_visible => {
                        ui_tiles.clear_bar(&mut ui);
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

/// Mini-map in the top-right corner: visited rooms, their unexplored
/// neighbours, the current room and (once known) the boss office.
fn show_minimap(frame: &mut GraphicsFrame, floor: &Floor, current: usize) {
    const CELL: i32 = 7;
    let x0 = agb::display::WIDTH - dungeon::GRID as i32 * CELL - 3;
    for (i, r) in floor.rooms.iter().enumerate() {
        let known = r.visited || (0..4).any(|d| floor.neighbour(i, d).is_some_and(|n| floor.rooms[n].visited));
        if !known {
            continue;
        }
        let tag = if i == current {
            &sprites::MM_CURRENT
        } else if i == floor.exit {
            &sprites::MM_EXIT
        } else if r.visited {
            &sprites::MM_ROOM
        } else {
            &sprites::MM_UNKNOWN
        };
        let mut o = Object::new(tag.sprite(0));
        o.set_pos((x0 + r.gx as i32 * CELL, 3 + r.gy as i32 * CELL));
        o.show(frame);
    }
}
