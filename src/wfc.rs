//! Simple-tiled Wave Function Collapse for office rooms.

use alloc::vec::Vec;

pub const W: usize = 15;
pub const H: usize = 10;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Terrain {
    Floor = 0,
    Carpet = 1,
    Tile = 2,
    Wall = 3,
    Box = 4,
    Stool = 5,
}

const N: usize = 6;
const ALL: u8 = (1 << N) - 1;
const TERRAINS: [Terrain; N] = [
    Terrain::Floor,
    Terrain::Carpet,
    Terrain::Tile,
    Terrain::Wall,
    Terrain::Box,
    Terrain::Stool,
];
const WEIGHTS: [u32; N] = [12, 5, 5, 2, 2, 2];

const fn bit(t: Terrain) -> u8 {
    1 << t as u8
}

/// For each terrain, the set of terrains allowed next to it (symmetric).
const ALLOWED: [u8; N] = [
    ALL,                                                                // floor
    bit(Terrain::Floor) | bit(Terrain::Carpet) | bit(Terrain::Stool),   // carpet
    bit(Terrain::Floor) | bit(Terrain::Tile) | bit(Terrain::Box),       // tile
    bit(Terrain::Floor) | bit(Terrain::Wall) | bit(Terrain::Box),       // wall
    bit(Terrain::Floor) | bit(Terrain::Tile) | bit(Terrain::Wall) | bit(Terrain::Box), // box
    bit(Terrain::Floor) | bit(Terrain::Carpet),                         // stool
];

impl Terrain {
    pub fn blocks(self) -> bool {
        matches!(self, Terrain::Wall | Terrain::Box | Terrain::Stool)
    }
}

/// Tiny xorshift RNG, seeded from the title-screen frame counter.
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(seed.wrapping_mul(2_654_435_761) | 1)
    }
    pub fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    pub fn range(&mut self, n: u32) -> u32 {
        self.next() % n
    }
}

pub type Map = [[Terrain; W]; H];

fn neighbours(x: usize, y: usize) -> impl Iterator<Item = (usize, usize)> {
    let mut v: [(usize, usize); 4] = [(usize::MAX, 0); 4];
    if x > 0 {
        v[0] = (x - 1, y);
    }
    if x + 1 < W {
        v[1] = (x + 1, y);
    }
    if y > 0 {
        v[2] = (x, y - 1);
    }
    if y + 1 < H {
        v[3] = (x, y + 1);
    }
    v.into_iter().filter(|p| p.0 != usize::MAX)
}

fn propagate(wave: &mut [[u8; W]; H], start: (usize, usize)) -> bool {
    let mut stack: Vec<(usize, usize)> = Vec::new();
    stack.push(start);
    while let Some((x, y)) = stack.pop() {
        let mut support = 0u8;
        for t in 0..N {
            if wave[y][x] & (1 << t) != 0 {
                support |= ALLOWED[t];
            }
        }
        for (nx, ny) in neighbours(x, y) {
            let before = wave[ny][nx];
            let after = before & support;
            if after == 0 {
                return false;
            }
            if after != before {
                wave[ny][nx] = after;
                stack.push((nx, ny));
            }
        }
    }
    true
}

fn try_generate(rng: &mut Rng) -> Option<Map> {
    let mut wave = [[ALL; W]; H];
    // Border is wall; interior can't be wall-free-floating noise near the edge.
    for y in 0..H {
        for x in 0..W {
            if x == 0 || y == 0 || x == W - 1 || y == H - 1 {
                wave[y][x] = bit(Terrain::Wall);
            }
        }
    }
    for y in 0..H {
        for x in 0..W {
            if !propagate(&mut wave, (x, y)) {
                return None;
            }
        }
    }

    loop {
        // Find the lowest-entropy undecided cell (random tie-break).
        let mut best: Option<(usize, usize)> = None;
        let mut best_count = u32::MAX;
        let mut ties = 0;
        for y in 0..H {
            for x in 0..W {
                let c = wave[y][x].count_ones();
                if c <= 1 {
                    continue;
                }
                if c < best_count {
                    best_count = c;
                    best = Some((x, y));
                    ties = 1;
                } else if c == best_count {
                    ties += 1;
                    if rng.range(ties) == 0 {
                        best = Some((x, y));
                    }
                }
            }
        }
        let Some((x, y)) = best else { break };

        let total: u32 = (0..N)
            .filter(|t| wave[y][x] & (1 << t) != 0)
            .map(|t| WEIGHTS[t])
            .sum();
        let mut pick = rng.range(total);
        let mut chosen = 0;
        for t in 0..N {
            if wave[y][x] & (1 << t) != 0 {
                if pick < WEIGHTS[t] {
                    chosen = t;
                    break;
                }
                pick -= WEIGHTS[t];
            }
        }
        wave[y][x] = 1 << chosen;
        if !propagate(&mut wave, (x, y)) {
            return None;
        }
    }

    let mut map = [[Terrain::Floor; W]; H];
    for y in 0..H {
        for x in 0..W {
            map[y][x] = TERRAINS[wave[y][x].trailing_zeros() as usize];
        }
    }
    Some(map)
}

pub fn generate(rng: &mut Rng) -> Map {
    for _ in 0..20 {
        if let Some(map) = try_generate(rng) {
            return map;
        }
    }
    let mut map = [[Terrain::Floor; W]; H];
    for y in 0..H {
        for x in 0..W {
            if x == 0 || y == 0 || x == W - 1 || y == H - 1 {
                map[y][x] = Terrain::Wall;
            }
        }
    }
    map
}

/// Flood-fill from `start`; unreachable walkable cells become boxes.
/// Returns the list of reachable cells.
pub fn make_connected(map: &mut Map, start: (usize, usize)) -> Vec<(usize, usize)> {
    let mut seen = [[false; W]; H];
    let mut stack = Vec::new();
    let mut reached = Vec::new();
    stack.push(start);
    seen[start.1][start.0] = true;
    while let Some((x, y)) = stack.pop() {
        reached.push((x, y));
        for (nx, ny) in neighbours(x, y) {
            if !seen[ny][nx] && !map[ny][nx].blocks() {
                seen[ny][nx] = true;
                stack.push((nx, ny));
            }
        }
    }
    for y in 0..H {
        for x in 0..W {
            if !seen[y][x] && !map[y][x].blocks() {
                map[y][x] = Terrain::Box;
            }
        }
    }
    reached
}
