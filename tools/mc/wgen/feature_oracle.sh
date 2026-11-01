#!/usr/bin/env bash
FEATURES=${FEATURES:-ore} exec bash "$(dirname "$0")/surface_oracle.sh" "$@"
