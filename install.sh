#!/bin/bash

cargo build --release --all-features

# figure out connector name
if [[ "$OSTYPE" == "darwin"* ]]; then
  FILENAME=target/release/libmemflow_daemon_connector.dylib
else
  FILENAME=target/release/libmemflow_daemon_connector.so
fi

if [ ! -z "$1" ] && [ $1 = "--system" ]; then
    # install daemon + cli
    if [[ "$OSTYPE" != "darwin"* ]]; then
        echo "installing/updating daemon service and cli tool"

        sudo systemctl stop memflow.service

        sudo cp target/release/memflow-cli /usr/bin/memflow
        sudo cp target/release/memflow-daemon /usr/bin/memflowd

        sudo mkdir -p /etc/memflow/
        sudo cp daemon.conf /etc/memflow/daemon.conf

        sudo cp memflow.service /etc/systemd/system/
        sudo systemctl enable memflow.service
        sudo systemctl start memflow.service
    fi

    # install connector to system dir
    echo "installing connector system-wide in /usr/lib/memflow"
    if [[ ! -d /usr/lib/memflow ]]; then
        sudo mkdir /usr/lib/memflow
    fi
    sudo cp $FILENAME /usr/lib/memflow
fi

# install connector in user dir
echo "installing connector for user in ~/.local/lib/memflow"
if [[ ! -d ~/.local/lib/memflow ]]; then
    mkdir -p ~/.local/lib/memflow
fi
cp $FILENAME ~/.local/lib/memflow
