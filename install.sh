#!/bin/bash

cargo build --release --all-features

sudo /bin/bash << EOF
systemctl stop memflow.service

cp target/release/memflow-cli /usr/local/bin/memflow
cp target/release/memflow-daemon /usr/local/bin/memflowd

mkdir /etc/memflow/
cp daemon.conf /etc/memflow/daemon.conf

cp memflow.service /etc/systemd/system/
systemctl enable memflow.service
systemctl start memflow.service
EOF
