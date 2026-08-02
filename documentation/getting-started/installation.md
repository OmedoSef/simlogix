# Installation

SimLogix is developed inside a devcontainer that bundles the Rust toolchain and the native libraries needed for its GUI (X11, OpenGL, GTK).

## Prerequisites

- [Docker](https://www.docker.com/)
- [VS Code](https://code.visualstudio.com/) with the [Dev Containers](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers) extension
- A Linux host with an X server (the GUI window is forwarded from the container to the host's display) and a D-Bus session bus / `xdg-desktop-portal` (needed for the native save/load file dialog to work from inside the container)

Before opening the devcontainer, allow local X11 connections from Docker on the host:

```bash
xhost +local:docker
```

## Opening the project

1. Open the repository folder in VS Code.
2. Run the command palette action **"Dev Containers: Reopen in Container"**.
3. Wait for the image to build (first time only) and the container to start.

The source code is bind-mounted into the container, not copied — edits made from the host or inside VS Code are reflected immediately.

## What's in the devcontainer image

- `rust:1-slim-bookworm` base image
- `clippy` and `rustfmt` components
- X11/OpenGL/GTK development libraries required by `eframe`/`egui` (`libx11-dev`, `libxkbcommon-dev`, `libxkbcommon-x11-dev`, `libgl1-mesa-dev`, `libgtk-3-dev`, and related XCB libs)

See [.devcontainer/Dockerfile](../../.devcontainer/Dockerfile) for the exact package list.

The host's X11 socket and D-Bus session socket are both bind-mounted into the container (see [devcontainer.json](../../.devcontainer/devcontainer.json)), so GUI windows and the native file dialog reach the host's real display/desktop portal instead of needing anything running inside the container.
