set unstable

# Display this help
default:
  @just --list --unsorted

# Configure all tools
setup:
  just zsh/setup
  just wez/setup
  just zellij/setup
  just nvim/setup
  just vcs/setup
  just nushell/setup


# Install all tools
install:
  just zsh/install
  just wez/install
  just zellij/install
  just nvim/install
  just vcs/install
  just tools/install

