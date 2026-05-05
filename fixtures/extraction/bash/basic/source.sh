#!/usr/bin/env bash

helper() {
    local value="$1"
    echo "$((value + 1))"
}

run_worker() {
    helper "$1"
}
