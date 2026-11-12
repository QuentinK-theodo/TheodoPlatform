//! Terrain grid + a small constrained Wave Function Collapse.
//!
//! Rooms come from hand-made templates (see `dungeon.rs`); WFC only fills the
//! decor zones marked `?`, constrained by the fixed cells around them.

use alloc::{vec, vec::Vec};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Terrain {
    Floor = 0,
    Carpet = 1,
    Tile = 2,
    Wall = 3,
    Box = 4,
    Plant = 5,
}

const N: usize = 6;
const ALL: u8 = (1 << N) - 1;
const TERRAINS: [Terrain; N] = [
    Terrain::Floor,
    Terrain::Carpet,
    Terrain::Tile,
    Terrain::Wall,
    Terrain::Box,
    Terrain::Plant,
];

const fn bit(t: Terrain) -> u8 {
    1 << t as u8
}

/// For each terrain, the set of terrains allowed next to it (symmetric).
const ALLOWED: [u8; N] = [
    ALL,                                                                               // floor
    bit(Terrain::Floor) | bit(Terrain::Carpet) | bit(Terrain::Plant),                  // carpet
    bit(Terrain::Floor) | bit(Terrain::Tile) | bit(Terrain::Box),                      // tile
    bit(Terrain::Floor) | bit(Terrain::Wall) | bit(Terrain::Box),                      // wall
    bit(Terrain::Floor) | bit(Terrain::Tile) | bit(Terrain::Wall) | bit(Terrain::Box), // box
    bit(Terrain::Floor) | bit(Terrain::Carpet),                                        // plant
];

impl Terrain {
    pub fn blocks(self) -> bool {
        matches!(self, Terrain::Wall | Terrain::Box | Terrain::Plant)
    }
}

/// Tiny xorshift RNG.
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

#[derive(Clone)]
pub struct Map {
    pub w: usize,
    pub h: usize,
    cells: Vec<Terrain>,
}

impl Map {
    pub fn new(w: usize, h: usize) -> Self {
        Map { w, h, cells: vec![Terrain::Floor; w * h] }
    }
    pub fn at(&self, x: usize, y: usize) -> Terrain {
        self.cells[y * self.w + x]
    }
    pub fn set(&mut self, x: usize, y: usize, t: Terrain) {
        self.cells[y * self.w + x] = t;
    }
    fn neighbours(&self, x: usize, y: usize) -> impl Iterator<Item = (usize, usize)> {
        let (w, h) = (self.w, self.h);
        [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ]
        .into_iter()
        .filter(move |&(nx, ny)| nx < w && ny < h)
    }
}

/// Fill the cells where `zone` is true using WFC, constrained by the fixed
/// cells around them. Falls back to plain floor if it keeps contradicting.
pub fn fill_zones(map: &mut Map, zone: &[bool], rng: &mut Rng, weights: &[u32; N]) {
    if !zone.iter().any(|&z| z) {
        return;
    }
    for _ in 0..10 {
        if try_fill(map, zone, rng, weights) {
            return;
        }
    }
    for (i, &z) in zone.iter().enumerate() {
        if z {
            map.cells[i] = Terrain::Floor;
        }
    }
}

fn try_fill(map: &mut Map, zone: &[bool], rng: &mut Rng, weights: &[u32; N]) -> bool {
    let (w, h) = (map.w, map.h);
    // No walls in decor zones: they'd read as broken architecture.
    let mut wave: Vec<u8> = (0..w * h)
        .map(|i| if zone[i] { ALL & !bit(Terrain::Wall) } else { bit(map.cells[i]) })
        .collect();

    let propagate = |wave: &mut Vec<u8>, start: usize| -> bool {
        let mut stack = vec![start];
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            let mut support = 0u8;
            for t in 0..N {
                if wave[i] & (1 << t) != 0 {
                    support |= ALLOWED[t];
                }
            }
            for (nx, ny) in map.neighbours(x, y) {
                let j = ny * w + nx;
                if !zone[j] {
                    continue; // fixed template cells are never changed
                }
                let after = wave[j] & support;
                if after == 0 {
                    return false;
                }
                if after != wave[j] {
                    wave[j] = after;
                    stack.push(j);
                }
            }
        }
        true
    };

    for i in 0..w * h {
        if !zone[i] && !propagate(&mut wave, i) {
            return false;
        }
    }

    loop {
        let mut best = None;
        let mut best_count = u32::MAX;
        let mut ties = 0;
        for i in 0..w * h {
            let c = wave[i].count_ones();
            if !zone[i] || c <= 1 {
                continue;
            }
            if c < best_count {
                best_count = c;
                best = Some(i);
                ties = 1;
            } else if c == best_count {
                ties += 1;
                if rng.range(ties) == 0 {
                    best = Some(i);
                }
            }
        }
        let Some(i) = best else { break };

        let total: u32 = (0..N).filter(|t| wave[i] & (1 << t) != 0).map(|t| weights[t]).sum();
        if total == 0 {
            return false;
        }
        let mut pick = rng.range(total);
        let mut chosen = 0;
        for t in 0..N {
            if wave[i] & (1 << t) != 0 {
                if pick < weights[t] {
                    chosen = t;
                    break;
                }
                pick -= weights[t];
            }
        }
        wave[i] = 1 << chosen;
        if !propagate(&mut wave, i) {
            return false;
        }
    }

    for i in 0..w * h {
        if zone[i] {
            map.cells[i] = TERRAINS[wave[i].trailing_zeros() as usize];
        }
    }
    true
}

/// Cells reachable on foot from `start`.
pub fn reachable(map: &Map, start: (usize, usize)) -> Vec<bool> {
    let mut seen = vec![false; map.w * map.h];
    let mut stack = vec![start];
    seen[start.1 * map.w + start.0] = true;
    while let Some((x, y)) = stack.pop() {
        for (nx, ny) in map.neighbours(x, y) {
            let j = ny * map.w + nx;
            if !seen[j] && !map.at(nx, ny).blocks() {
                seen[j] = true;
                stack.push((nx, ny));
            }
        }
    }
    seen
}
