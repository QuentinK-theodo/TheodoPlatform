# Theodo RPG

Démo GBA en Rust ([agb](https://agbrs.dev) 0.25) : un RPG façon Mario & Luigi en version roguelite, avec des salles de bureau générées par **Wave Function Collapse** (`src/wfc.rs`).

- Exploration : les flèches pour bouger. Toucher un ennemi lance le combat.
- Combat : **A** quand le curseur est au centre = coup critique ; **A** au moment de l'impact = esquive.
- Toutes les salles vidées → nouvelle salle (nouvelle seed, ennemis plus forts). Mort → retour à la salle 1.

## Lancer

```bash
cargo run --release          # ouvre mgba-qt
```

Produire la ROM : `agb-gbafix target/thumbv4t-none-eabi/release/theodo_rpg -o theodo_rpg.gba`.
GitHub Pages : `.github/workflows/pages.yml` build la ROM et la sert avec EmulatorJS (`web/index.html`).
Activer Pages avec la source « GitHub Actions » dans les settings du repo.

## Assets

`tools/extract.py` convertit le pack LimeZu *Modern Interiors* (free, usage non commercial, voir `gfx/LICENSE-LimeZu.txt`), à placer dans `Modern tiles_Free/`. Police : Ark Pixel (OFL).
