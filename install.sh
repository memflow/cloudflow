#!/bin/bash

cargo build --release --all-features

if [[ "$OSTYPE" == "darwin"* ]]; then
  FILENAME=target/release/libmemflow_daemon_connector.dylib
else
  FILENAME=target/release/libmemflow_daemon_connector.so
fi

if [ ! -z "$1" ] && [ $1 = "--system" ]; then
    if [[ ! -d /usr/local/lib/memflow ]]; then
        sudo mkdir /usr/local/lib/memflow
    fi
    sudo cp $FILENAME /usr/local/lib/memflow
else
    if [[ ! -d ~/.local/lib/memflow ]]; then
        mkdir -p ~/.local/lib/memflow
    fi
    cp $FILENAME ~/.local/lib/memflow
fi
