#!/bin/sh
# Reapply only DeepCool hidraw rules; do not reset USB hardware.
if command -v udevadm >/dev/null 2>&1; then
    if [ "${1-}" != --dry-run ]; then
        udevadm control --reload-rules || true
    fi
    for device in /sys/class/hidraw/hidraw*; do
        [ -e "$device" ] || continue
        parent=$(readlink -f "$device")
        while [ "$parent" != / ] && [ -n "$parent" ]; do
            if [ -r "$parent/idVendor" ] && [ -r "$parent/idProduct" ]; then
                vendor=$(cat "$parent/idVendor")
                product=$(cat "$parent/idProduct")
                case "$vendor:$product" in
                    3633:*|34d3:1100)
                        if [ "${1-}" = --dry-run ]; then
                            udevadm trigger --dry-run --verbose --action=change "$device" || true
                        else
                            udevadm trigger --action=change "$device" || true
                        fi
                        ;;
                esac
                break
            fi
            parent=${parent%/*}
        done
    done
    if [ "${1-}" != --dry-run ]; then
        udevadm settle --timeout=10 || true
    fi
fi
