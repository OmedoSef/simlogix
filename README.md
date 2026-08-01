# SimLogix

Simulateur logique multiplateforme, écrit en Rust — pensé pour corriger les frustrations rencontrées avec Logisim (interaction canevas peu fluide, boucles rétroactives mal gérées, UI/UX datée).

## Prérequis

- Docker + VS Code avec l'extension [Dev Containers](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers) — le projet se développe via un devcontainer qui embarque la toolchain Rust.
- Sur l'hôte, avant d'ouvrir le devcontainer, autoriser les connexions X11 locales pour que la fenêtre de l'app puisse s'afficher depuis le conteneur :

  ```bash
  xhost +local:docker
  ```

## Démarrer

1. Ouvrir le dossier dans VS Code.
2. Palette de commandes → "Dev Containers: Reopen in Container".
3. _(à venir une fois le workspace Cargo scaffoldé)_ `cargo run -p simlogix-gui`

## Documentation

Contexte du projet, décisions d'architecture et avancement : voir [CLAUDE.md](CLAUDE.md).
