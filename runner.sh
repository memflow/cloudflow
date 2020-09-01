#!/bin/bash

echo $@

PCMD=$(cat /proc/$PPID/cmdline | strings -1)
PCMD=$(echo -n ${PCMD##*/})
RNAME=$(echo -n ${@##*/})

if [[ "$PCMD" =~ "cargo test" ]]; then
	exec $@
elif [[ "$PCMD" =~ "cargo bench" ]]; then
	exec $@
else
	if [[ $RNAME =~ "memflow-daemon" ]]; then
		exec sudo PATH=$PATH $@
	else
		exec $@
	fi
fi
