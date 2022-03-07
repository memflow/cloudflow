# cloudflow

![Build and test](https://github.com/memflow/memflow-cli/workflows/Build%20and%20test/badge.svg?branch=master)
[![codecov](https://codecov.io/gh/memflow/memflow-cli/branch/master/graph/badge.svg?token=XT7R158N6W)](https://codecov.io/gh/memflow/memflow-cli)

Make memflow scale.

## Pluggable framework and UI for memflow

This project aims to be an extensible framework for memflow applications. Adding new features should require as least boilerplate as possible, and accessing them should be as trivial as possible.

### Features

This project is currently in its infancy, but it already has the following features:

* FUSE interface.

* Full connector/os chaining.

* Process information.

* Standalone minidump generator.

### How to install

Building from source:

```
cargo install cloudflow-node --git https://github.com/memflow/cloudflow
```

### How to use

Run an elevated instance with FUSE (you can add `v` multiple time to increase verbosity e.g. `-vvv`):

```
cloudflow -ef
```

You should be able to see the following messages:

```
Mounting FUSE filesystem on /cloudflow
Initialized!
```

Create a new connector instance to connect to the first QEMU VM instance which can be found on your system:

```
echo "my_qemu_vm qemu" >> /cloudflow/connector/new
```

To connect to a specific VM with the name 'my-qemu-vm' you can pass an argument to the connector:

```
echo "my_qemu_vm qemu:my-qemu-vm" >> /cloudflow/connector/new
```

Create a new OS instance on top of QEMU:

```
echo "win -c my_qemu_vm win32" >> /cloudflow/os/new
```

Optionally you can specify the architecture for the new OS instance:

```
echo "win -c my_qemu_vm win32::arch=x64" >> /cloudflow/os/new
```

The input format can contain args and extra args. Both are parsed by the new connector/OS instance. The available options therefore depend on the type of the new instance. For all of the operations above the input format is as follows:

```
<name> [-c chain_on] <os/connector>[:args[:extra args]]
```

Get kernel minidump:

```
cat /cloudflow/os/win/processes/by-name/System/mini.dmp > System.dmp
```

## Contributing

Please check [CONTRIBUTE.md](CONTRIBUTE.md)
