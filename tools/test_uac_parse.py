#!/usr/bin/env python3
"""Teste host do parser UAC (espelha audio/usb.rs::parse_config_for_audio)."""


def parse_config_for_audio(cfg: bytes):
    if len(cfg) < 9 or cfg[1] != 0x02:
        return None
    total = cfg[2] | (cfg[3] << 8)
    end = min(total, len(cfg))
    has_ac = has_as = False
    cap_ep = play_ep = 0
    i = 0
    in_as = False
    while i + 2 <= end:
        blen = cfg[i]
        if blen < 2 or i + blen > end:
            break
        dtype = cfg[i + 1]
        if dtype == 0x04 and blen >= 9:
            if_class, if_sub = cfg[i + 5], cfg[i + 6]
            in_as = False
            if if_class == 0x01:
                if if_sub == 0x01:
                    has_ac = True
                elif if_sub == 0x02:
                    has_as = True
                    in_as = True
        elif dtype == 0x05 and blen >= 7 and in_as:
            addr, attr = cfg[i + 2], cfg[i + 3]
            if (attr & 0x03) == 0x01:
                if addr & 0x80:
                    cap_ep = addr
                else:
                    play_ep = addr
        i += blen
    if has_ac or has_as:
        return {"ac": has_ac, "as": has_as, "cap": cap_ep, "play": play_ep}
    return None


def main():
    cfg = bytearray(
        [
            9,
            0x02,
            0,
            0,
            2,
            1,
            0,
            0x80,
            50,
            9,
            0x04,
            0,
            0,
            0,
            0x01,
            0x01,
            0x00,
            0,
            9,
            0x04,
            1,
            0,
            2,
            0x01,
            0x02,
            0x00,
            0,
            7,
            0x05,
            0x81,
            0x01,
            0xC0,
            0x00,
            1,
            7,
            0x05,
            0x01,
            0x01,
            0xC0,
            0x00,
            1,
        ]
    )
    cfg[2] = len(cfg) & 0xFF
    cfg[3] = (len(cfg) >> 8) & 0xFF
    info = parse_config_for_audio(bytes(cfg))
    assert info is not None
    assert info["ac"] and info["as"]
    assert info["cap"] == 0x81 and info["play"] == 0x01
    print("[OK] UAC config parse")


if __name__ == "__main__":
    main()
