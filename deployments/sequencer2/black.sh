#!/bin/bash

# Run black on the sequencer deployment project.

DEFAULT_ARGS="-l 100 -t py310 --exclude imports"
function fix() {
    black ${DEFAULT_ARGS} .
}

function check() {
    black --diff --color  ${DEFAULT_ARGS} .
}

[[ $1 == "--fix" ]] && fix
[[ $1 == "--check" ]] && check
