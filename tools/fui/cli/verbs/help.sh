# SPDX-License-Identifier: MPL-2.0
verb_help() {
    cat <<USAGE
usage:
  fui new <name> [--dir DIR]
  fui build [DIR] [--targets native,web,android] [--out DIR] [--verify] [--limit PCT] [--opt LEVEL] [--abi ABI]
  fui verify [DIR] [--out DIR] [--limit PCT]
  fui targets
exit codes: 0 ok, 1 build failed, 2 usage, 3 pixel gate failed, 4 target/tool unavailable, 5 manifest error
USAGE
}
