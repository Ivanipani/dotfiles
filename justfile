set unstable

# Display this help
default:
  @just --list --unsorted

# Configure all tools
[macos]
setup:
  just term/setup
  just zellij/setup
  just nvim/setup
  just vcs/setup
  just nushell/setup
  just zsh/setup


# Install all tools
[macos]
install:
  just wez/install
  just zellij/install
  just nvim/install
  just vcs/install
  just tools/install
  just zsh/install

# Configure all tools
[linux]
setup:
  just zsh/setup
  just zellij/setup
  just nvim/setup
  just vcs/setup

# Install all tools
[linux]
install:
  just zsh/install
  just zellij/install
  just nvim/install
  just vcs/install
  just tools/install

