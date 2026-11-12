//! Hand-made room templates + procedural floor layout.
//!
//! A floor is a set of rooms on a 5x5 grid, grown by random walk from the
//! start room. Each room is a template (ASCII below), optionally mirrored,
//! with WFC filling its `?` decor zones and enemies placed on random `E` cells.

use alloc::vec::Vec;

use crate::wfc::{self, Map, Rng, Terrain};

/// Legend: `#` wall, `.` floor, `~` carpet, `T` tiled floor, `B` box/desk,
/// `P` plant, `E` possible enemy spot (floor), `?` WFC decor zone.
/// Doors are punched in the middle of each wall that has a neighbour room.
pub struct Template {
    pub name: &'static str,
    /// Index of this room's floor in gfx/tiles.png.
    pub floor_tile: usize,
    /// WFC weights for `?` zones: floor, carpet, tile, wall, box, plant.
    pub weights: [u32; 6],
    pub rows: &'static [&'static str],
}

const FLOOR_YELLOW: usize = 0;
const FLOOR_RED: usize = 8;
const FLOOR_GREY: usize = 9;

pub const START: usize = 0;
pub const BOSS: usize = 1;
/// Index of the first regular (fight) template.
const FIRST_REGULAR: usize = 2;

pub static TEMPLATES: &[Template] = &[
    Template {
        name: "Accueil",
        floor_tile: FLOOR_YELLOW,
        weights: [6, 2, 0, 0, 1, 2],
        rows: &[
            "################",
            "#P............P#",
            "#..~~~~~~~~~~..#",
            "#..~........~..#",
            "#..~...??...~..#",
            "#..~...??...~..#",
            "#..~........~..#",
            "#..~~~~~~~~~~..#",
            "#P............P#",
            "################",
        ],
    },
    Template {
        name: "Bureau du boss",
        floor_tile: FLOOR_RED,
        weights: [6, 3, 0, 0, 1, 1],
        rows: &[
            "####################",
            "#P................P#",
            "#..................#",
            "#..~~~~~~~~~~~~~~..#",
            "#..~............~..#",
            "#..~...BBBBBB...~..#",
            "#..~.....E......~..#",
            "#..~............~..#",
            "#..~...E....E...~..#",
            "#..~~~~~~~~~~~~~~..#",
            "#..................#",
            "#P................P#",
            "#..................#",
            "####################",
        ],
    },
    Template {
        name: "Open space",
        floor_tile: FLOOR_YELLOW,
        weights: [10, 4, 0, 0, 3, 3],
        rows: &[
            "##############################",
            "#............................#",
            "#.~~~~~~~~...P......~~~~~~~~.#",
            "#.~BB~BB~~..........~BB~BB~~.#",
            "#.~~~~~~~~....E.....~~~~~~~~.#",
            "#..........................P.#",
            "#P....??????......??????.....#",
            "#.....??????..E...??????.....#",
            "#.....??????......??????..E..#",
            "#............................#",
            "#.~~~~~~~~..........~~~~~~~~.#",
            "#.~BB~BB~~....E.....~BB~BB~~.#",
            "#.~~~~~~~~..........~~~~~~~~.#",
            "#..P.....................P...#",
            "#............................#",
            "##############################",
        ],
    },
    Template {
        name: "Cuisine",
        floor_tile: FLOOR_RED,
        weights: [8, 0, 6, 0, 3, 1],
        rows: &[
            "#################",
            "#TTTTBBBBBBTTTTT#",
            "#TTTTTTTTTTTTTTT#",
            "#TT.....E....TTT#",
            "#TT.BB....BB.TTT#",
            "#TT..........TTT#",
            "#TT.BB....BB.TTT#",
            "#TT....E.....TTT#",
            "#TTTTTTTTTTTTTTT#",
            "#P....E.......P.#",
            "#################",
        ],
    },
    Template {
        name: "Salle de reunion",
        floor_tile: FLOOR_GREY,
        weights: [8, 4, 0, 0, 1, 2],
        rows: &[
            "####################",
            "#P........E.......P#",
            "#..................#",
            "#..~~~~~~~~~~~~~~..#",
            "#..~BBBBBBBBBBBB~..#",
            "#E.~BBBBBBBBBBBB~.E#",
            "#..~~~~~~~~~~~~~~..#",
            "#..................#",
            "#.....E......E.....#",
            "#P................P#",
            "#..................#",
            "####################",
        ],
    },
    Template {
        name: "Archives",
        floor_tile: FLOOR_GREY,
        weights: [8, 0, 1, 0, 5, 1],
        rows: &[
            "########################",
            "#......................#",
            "#.BBBBBB.BBBBBB.BBBBBB.#",
            "#......................#",
            "#.BBBBBB.BBBBBB.BBBBBB.#",
            "#.....E.........E......#",
            "#.BBBBBB.??????.BBBBBB.#",
            "#........??????........#",
            "#.BBBBBB.??????.BBBBBB.#",
            "#......E...............#",
            "#.BBBBBB.BBBBBB.BBBBBB.#",
            "#......................#",
            "#.BBBBBB.BBBBBB.BBBBBB.#",
            "#P.........E..........P#",
            "#......................#",
            "########################",
        ],
    },
    Template {
        name: "Salle serveurs",
        floor_tile: FLOOR_GREY,
        weights: [6, 0, 6, 0, 3, 0],
        rows: &[
            "##################",
            "#................#",
            "#.##.##.##.##.##.#",
            "#.##.##.##.##.##.#",
            "#.......E........#",
            "#TTTTTTTTTTTTTTTT#",
            "#TTTT?????TTTTTTT#",
            "#TTTT?????TTT.E.T#",
            "#.......E........#",
            "#.##.##.##.##.##.#",
            "#P..............P#",
            "##################",
        ],
    },
    Template {
        name: "Lounge",
        floor_tile: FLOOR_YELLOW,
        weights: [8, 5, 0, 0, 1, 3],
        rows: &[
            "######################",
            "#P..................P#",
            "#..~~~~~~....~~~~~~..#",
            "#..~PP~~~....~~~PP~..#",
            "#..~~~~~~.E..~~~~~~..#",
            "#....................#",
            "#.E....????????....E.#",
            "#......????????......#",
            "#....................#",
            "#..~~~~~~....~~~~~~..#",
            "#..~~BB~~.E..~~BB~~..#",
            "#..~~~~~~....~~~~~~..#",
            "#P..................P#",
            "######################",
        ],
    },
];

// --- Floor layout ------------------------------------------------------------

pub const GRID: usize = 5;

/// Door directions, in this order everywhere: up, right, down, left.
pub const DIRS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

pub struct RoomSlot {
    pub gx: usize,
    pub gy: usize,
    pub template: usize,
    pub mirror: bool,
    pub seed: u32,
    pub cleared: bool,
    pub visited: bool,
}

pub struct Floor {
    pub rooms: Vec<RoomSlot>,
    pub grid: [[Option<usize>; GRID]; GRID],
    pub exit: usize,
}

impl Floor {
    pub fn neighbour(&self, room: usize, dir: usize) -> Option<usize> {
        let r = &self.rooms[room];
        let (dx, dy) = DIRS[dir];
        let (x, y) = (r.gx as i32 + dx, r.gy as i32 + dy);
        if x < 0 || y < 0 || x >= GRID as i32 || y >= GRID as i32 {
            return None;
        }
        self.grid[y as usize][x as usize]
    }
}

/// Grow `count` rooms by random walk from the centre; the exit is the room
/// farthest (in steps) from the start.
pub fn generate_floor(rng: &mut Rng, count: usize) -> Floor {
    let mut grid = [[None; GRID]; GRID];
    let mut cells: Vec<(usize, usize)> = Vec::new();
    let c = GRID / 2;
    grid[c][c] = Some(0);
    cells.push((c, c));

    let mut guard = 0;
    while cells.len() < count && guard < 1000 {
        guard += 1;
        let (x, y) = cells[rng.range(cells.len() as u32) as usize];
        let (dx, dy) = DIRS[rng.range(4) as usize];
        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
        if nx < 0 || ny < 0 || nx >= GRID as i32 || ny >= GRID as i32 {
            continue;
        }
        let (nx, ny) = (nx as usize, ny as usize);
        if grid[ny][nx].is_some() {
            continue;
        }
        // Prefer corridors-and-branches over blobs: skip cells with 2+ neighbours.
        let around = DIRS
            .iter()
            .filter(|(ddx, ddy)| {
                let (ax, ay) = (nx as i32 + ddx, ny as i32 + ddy);
                ax >= 0 && ay >= 0 && ax < GRID as i32 && ay < GRID as i32 && grid[ay as usize][ax as usize].is_some()
            })
            .count();
        if around >= 2 && guard < 800 {
            continue;
        }
        grid[ny][nx] = Some(cells.len());
        cells.push((nx, ny));
    }

    // BFS distances from the start to pick the exit.
    let mut dist = [[u32::MAX; GRID]; GRID];
    dist[c][c] = 0;
    let mut queue = alloc::collections::VecDeque::new();
    queue.push_back((c, c));
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in DIRS {
            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
            if nx < 0 || ny < 0 || nx >= GRID as i32 || ny >= GRID as i32 {
                continue;
            }
            let (nx, ny) = (nx as usize, ny as usize);
            if grid[ny][nx].is_some() && dist[ny][nx] == u32::MAX {
                dist[ny][nx] = dist[y][x] + 1;
                queue.push_back((nx, ny));
            }
        }
    }
    let exit = (1..cells.len()).max_by_key(|&i| dist[cells[i].1][cells[i].0]).unwrap_or(0);

    let regular = TEMPLATES.len() - FIRST_REGULAR;
    let mut last = usize::MAX;
    let rooms = cells
        .iter()
        .enumerate()
        .map(|(i, &(gx, gy))| {
            let template = if i == 0 {
                START
            } else if i == exit {
                BOSS
            } else {
                // Avoid the same template twice in a row.
                let mut t = FIRST_REGULAR + rng.range(regular as u32) as usize;
                if t == last {
                    t = FIRST_REGULAR + (t - FIRST_REGULAR + 1) % regular;
                }
                last = t;
                t
            };
            RoomSlot {
                gx,
                gy,
                template,
                mirror: rng.range(2) == 0,
                seed: rng.next(),
                cleared: i == 0,
                visited: i == 0,
            }
        })
        .collect();

    Floor { rooms, grid, exit }
}

// --- Room instantiation ------------------------------------------------------

pub struct BuiltRoom {
    pub map: Map,
    /// Door cell for each direction (up, right, down, left), if there is a neighbour.
    pub doors: [Option<(usize, usize)>; 4],
    pub spawns: Vec<(usize, usize)>,
}

impl BuiltRoom {
    /// The walkable cell just inside a door.
    pub fn inside(&self, dir: usize) -> (usize, usize) {
        let (x, y) = self.doors[dir].unwrap_or((self.map.w / 2, self.map.h / 2));
        let (dx, dy) = DIRS[dir];
        ((x as i32 - dx) as usize, (y as i32 - dy) as usize)
    }
}

/// Build a room's terrain from its template. Deterministic for a given slot,
/// so a room looks the same when you come back.
pub fn build_room(floor: &Floor, room: usize) -> BuiltRoom {
    let slot = &floor.rooms[room];
    let t = &TEMPLATES[slot.template];
    let mut rng = Rng::new(slot.seed);
    let h = t.rows.len();
    let w = t.rows[0].len();
    let mut map = Map::new(w, h);
    let mut zone = alloc::vec![false; w * h];
    let mut spawns = Vec::new();

    for (y, row) in t.rows.iter().enumerate() {
        for (x, ch) in row.bytes().enumerate() {
            let x = if slot.mirror { w - 1 - x } else { x };
            let terrain = match ch {
                b'#' => Terrain::Wall,
                b'~' => Terrain::Carpet,
                b'T' => Terrain::Tile,
                b'B' => Terrain::Box,
                b'P' => Terrain::Plant,
                b'?' => {
                    zone[y * w + x] = true;
                    Terrain::Floor
                }
                b'E' => {
                    spawns.push((x, y));
                    Terrain::Floor
                }
                _ => Terrain::Floor,
            };
            map.set(x, y, terrain);
        }
    }
    wfc::fill_zones(&mut map, &zone, &mut rng, &t.weights);

    // Doors in the middle of each wall that leads somewhere, with a guaranteed
    // straight path from the door to the room centre.
    let centre = (w / 2, h / 2);
    let mut doors = [None; 4];
    for dir in 0..4 {
        if floor.neighbour(room, dir).is_none() {
            continue;
        }
        let door = match dir {
            0 => (w / 2, 0),
            1 => (w - 1, h / 2),
            2 => (w / 2, h - 1),
            _ => (0, h / 2),
        };
        doors[dir] = Some(door);
        map.set(door.0, door.1, Terrain::Wall); // drawn as a door, see main.rs
    }
    if map.at(centre.0, centre.1).blocks() {
        map.set(centre.0, centre.1, Terrain::Floor);
    }
    for dir in 0..4 {
        let Some(door) = doors[dir] else { continue };
        let (dx, dy) = DIRS[dir];
        let (mut x, mut y) = ((door.0 as i32 - dx) as usize, (door.1 as i32 - dy) as usize);
        // Walk inward until we join the area reachable from the centre.
        let from_centre = wfc::reachable(&map, centre);
        while !from_centre[y * w + x] && (x, y) != centre {
            map.set(x, y, Terrain::Floor);
            if x != centre.0 && dy == 0 {
                x = (x as i32 - dx) as usize;
            } else if y != centre.1 && dx == 0 {
                y = (y as i32 - dy) as usize;
            } else {
                break;
            }
        }
        map.set(x, y, Terrain::Floor);
    }

    // Keep only spawns the hero can actually reach.
    let ok = wfc::reachable(&map, centre);
    spawns.retain(|&(x, y)| ok[y * w + x]);

    BuiltRoom { map, doors, spawns }
}
