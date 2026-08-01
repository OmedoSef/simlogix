# SimLogix

> Ce fichier est le guide de référence du projet. Il doit être **mis à jour à chaque nouvelle décision** (architecture, périmètre, convention) pour rester la source de vérité — y compris quand on change de machine.

Nom du projet : **SimLogix** (Sim + Logic).

## Contexte du projet

Romain veut remplacer Logisim par un outil qui corrige ses frustrations principales :

- **Interaction canevas laborieuse** : placer/relier portes et fils est lent et peu fluide.
- **Apparence des composants pénible à créer** : l'éditeur de forme de Logisim est limité.
- **Boucles rétroactives mal gérées** : les circuits avec feedback combinatoire (ex. bascule SR en NAND) provoquent des bugs/blocages dans le moteur de simulation.
- **UI/UX datée** (Swing/Java).

Objectif : un simulateur logique multiplateforme, écrit en Rust.

## Décisions d'architecture

_(état actuel — rien n'est encore implémenté, voir Avancement)_

- **Workspace Cargo à 2 crates** :
  - `simlogix-core/` — modèle de circuit + moteur de simulation, sans dépendance GUI, testable en isolation.
  - `simlogix-gui/` — éditeur de schéma + rendu + boucle de simulation temps réel.
- **GUI : egui / eframe.** Choisi plutôt qu'iced ou Slint pour le contrôle direct du `Painter` (rendu custom fluide des portes/fils/grille) et le mode immédiat qui colle naturellement à une simulation temps réel. Cross-platform natif + export WASM possible plus tard.
- **Moteur de simulation : événements discrets avec délai de propagation.** C'est la réponse au problème des boucles rétroactives. Chaque composant a un délai de propagation (par défaut 1 tick logique) ; un changement d'entrée planifie un événement de sortie à `t + délai` au lieu de se propager instantanément. Une file d'événements ordonnée par temps logique traite les événements dans l'ordre — comme les simulateurs HDL (Verilog). Ça évite par construction la récursion infinie sur une boucle combinatoire (bascule SR-NAND, oscillateur en anneau converge/oscille au lieu de planter).
  - Détection d'oscillation : si un net change d'état plus de N fois dans le même pas de temps, le moteur arrête et signale "circuit instable" au lieu de geler l'UI.
- **Modèle de données** :
  - `Signal::{High, Low, Unknown, Error}` — l'état inconnu/erreur est prévu dès le départ (un des irritants "bugs de simulation" vient souvent d'un mauvais traitement de X/Z).
  - `Pin` : entrée/sortie d'un composant, connectée à un `Net`.
  - `Component` : trait avec `eval(&self, inputs) -> outputs` + `propagation_delay()`. Les sous-circuits implémentent aussi ce trait (hiérarchie = citoyen de première classe, pas un hack).
  - `Circuit` : graphe de composants + nets, file d'événements, horloge logique.
- **Dev container** (`.devcontainer/`) : image `rust:1-slim-bookworm` + libs X11/GL/GTK nécessaires à eframe (`libx11-dev`, `libxkbcommon-dev`, `libgl1-mesa-dev`, `libgtk-3-dev` pour les futurs dialogues de fichiers natifs via `rfd`, etc.), `clippy`/`rustfmt` installés. Le socket X11 de l'hôte (`/tmp/.X11-unix`) est monté et `DISPLAY` propagé, pour que `cargo run` depuis le conteneur ouvre la fenêtre directement sur le bureau hôte (X11 forwarding). `remoteUser: vscode` pour rester cohérent avec le pattern déjà utilisé sur `file-checker`. Le conteneur ne sert que pour la toolchain de build/dev (le code source est monté en volume par VS Code, pas copié dans l'image).
  - Instructions pratiques de setup (prérequis `xhost`, comment ouvrir le devcontainer) : voir [README.md](README.md), pas dupliquées ici.

## Périmètre v1 / Hors-scope

**Dans la v1 (simulateur minimal) :**
- Portes de base : AND, OR, NOT, NAND, NOR, XOR, XNOR, buffer.
- Entrée (switch/bouton), sortie (LED), horloge.
- Édition de schéma : placement, tracé de fils avec snapping à la grille et routage orthogonal, rotation, sélection/déplacement multiple.
- Simulation temps réel branchée sur la boucle UI.
- Sauvegarde/chargement de circuit (serde).

**Hors-scope v1 (roadmap future, pas à construire maintenant) :**
- Éditeur d'apparence/symbole personnalisé pour un composant — v1 utilise une apparence auto-générée (boîte + pins nommés). C'est le point de douleur n°2 de Romain, traité après que le noyau soit solide.
- Bus multi-bits, mémoire (RAM/ROM), export VHDL/FPGA, collaboration.

## Conventions de travail

- **On avance ensemble, petit à petit.** Ne pas scaffolder ou coder de gros blocs d'un coup sans validation — proposer une étape, discuter, puis implémenter.
- Ce fichier doit être mis à jour dès qu'une nouvelle décision structurante est prise (architecture, périmètre, convention) — pas seulement en fin de session.
- Toujours vérifier que ce fichier reflète l'état réel du code avant de s'y fier pleinement (le code fait foi en cas de divergence).

## Avancement

- [x] Cadrage du projet et de l'architecture (ce document).
- [x] Scaffold git + devcontainer (toolchain Rust, X11 GUI passthrough).
- [x] Renommer le dossier/repo `new-logisim` → `simlogix`.
- [x] README.md (setup pratique) séparé de CLAUDE.md (décisions/contexte).
- [ ] Scaffold du workspace Cargo.
- [ ] Moteur `simlogix-core` (modèle de données + événements discrets).
- [ ] Tests boucles rétroactives (bascule SR-NAND, oscillateur en anneau).
- [ ] Shell GUI eframe/egui minimal.
- [ ] Interaction éditeur (placement, tracé de fils, snapping, rotation, sélection).
- [ ] Intégration simulation temps réel ↔ UI.
- [ ] Sauvegarde/chargement de circuit.
