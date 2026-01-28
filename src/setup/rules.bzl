def _qemu_run_impl(ctx):
    runner = ctx.actions.declare_output("qemu_runner.sh")

    kernel = ctx.attrs.kernel[DefaultInfo].default_outputs[0]
    disk = ctx.attrs.disk

    # Create the command line, ensuring spaces between arguments
    command = cmd_args(
        "qemu-system-aarch64",
        "-M", "virt,highmem=off",
        "-cpu", "cortex-a53",
        "-m", "1024",
        "-kernel", kernel,
        "-drive", cmd_args(disk, format="if=none,file={},id=hd0,format=raw,file.locking=off"),
        "-device", "virtio-blk-pci,drive=hd0",
        "-serial", "stdio",
        "-display", "none",
        "\"$@\"",
        delimiter=" ",
    )

    # Write the script with shebang and command on separate lines
    ctx.actions.write(
        runner,
        cmd_args("#!/bin/bash", command, delimiter="\n"),
        is_executable = True
    )

    return [
        DefaultInfo(default_output = runner),
        RunInfo(args = cmd_args(runner, hidden = [kernel, disk])),
    ]

qemu_run = rule(
    impl = _qemu_run_impl,
    attrs = {
        "kernel": attrs.dep(),
        "disk": attrs.source(),
    },
)
