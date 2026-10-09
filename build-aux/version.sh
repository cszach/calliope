#!/bin/sh
# Prints Calliope's version. It lives in Cargo.toml only; a test checks the
# metainfo's newest release against it.
sed -n 's/^version = "\(.*\)"$/\1/p' "$(dirname "$0")/../Cargo.toml" | head -n 1
