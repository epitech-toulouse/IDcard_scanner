#!/usr/bin/env python3
"""Test ACR122U via libusb (ctypes), sans pcscd ni dépendance Python."""
import ctypes, sys, time

VID, PID = 0x072F, 0x2200
EP_OUT, EP_IN = 0x02, 0x82
DURATION = float(sys.argv[1]) if len(sys.argv) > 1 else 30

lib = ctypes.CDLL("libusb-1.0.so.0")
lib.libusb_open_device_with_vid_pid.restype = ctypes.c_void_p
lib.libusb_open_device_with_vid_pid.argtypes = [ctypes.c_void_p, ctypes.c_uint16, ctypes.c_uint16]
for f in ("libusb_kernel_driver_active", "libusb_detach_kernel_driver",
          "libusb_attach_kernel_driver", "libusb_claim_interface", "libusb_release_interface"):
    getattr(lib, f).argtypes = [ctypes.c_void_p, ctypes.c_int]
lib.libusb_bulk_transfer.argtypes = [ctypes.c_void_p, ctypes.c_ubyte, ctypes.c_void_p,
                                     ctypes.c_int, ctypes.POINTER(ctypes.c_int), ctypes.c_uint]
lib.libusb_close.argtypes = [ctypes.c_void_p]

ctx = ctypes.c_void_p()
assert lib.libusb_init(ctypes.byref(ctx)) == 0
h = lib.libusb_open_device_with_vid_pid(ctx, VID, PID)
if not h:
    sys.exit("Lecteur ACR122U introuvable / accès refusé")

detached = False
if lib.libusb_kernel_driver_active(h, 0) == 1:
    r = lib.libusb_detach_kernel_driver(h, 0)
    if r != 0:
        sys.exit(f"Impossible de détacher le pilote noyau (err {r})")
    detached = True
r = lib.libusb_claim_interface(h, 0)
if r != 0:
    sys.exit(f"claim_interface échoué (err {r})")

seq = 0
def ccid(msg_type, payload=b"", timeout=3000):
    global seq
    hdr = bytes([msg_type]) + len(payload).to_bytes(4, "little") + bytes([0, seq & 0xFF, 0, 0, 0])
    seq += 1
    out = hdr + payload
    n = ctypes.c_int()
    buf = (ctypes.c_ubyte * len(out)).from_buffer_copy(out)
    r = lib.libusb_bulk_transfer(h, EP_OUT, buf, len(out), ctypes.byref(n), timeout)
    if r != 0:
        raise IOError(f"bulk OUT err {r}")
    inbuf = (ctypes.c_ubyte * 512)()
    while True:
        r = lib.libusb_bulk_transfer(h, EP_IN, inbuf, 512, ctypes.byref(n), timeout)
        if r != 0:
            raise IOError(f"bulk IN err {r}")
        resp = bytes(inbuf[:n.value])
        if len(resp) >= 10 and resp[7] & 0xC0 == 0x80:  # time extension
            continue
        return resp[10:10 + int.from_bytes(resp[1:5], "little")]

def escape(apdu):  # PC_to_RDR_Escape -> commandes directes au lecteur
    return ccid(0x6B, bytes(apdu))

def pn532(*cmd):
    return escape([0xFF, 0x00, 0x00, 0x00, len(cmd) + 1, 0xD4, *cmd])

def beep_led(ok=True):
    # LED verte/rouge clignotante + bip court
    state = 0x2E if ok else 0x5D
    escape([0xFF, 0x00, 0x40, state, 0x04, 0x01, 0x01, 0x01, 0x01])

try:
    fw = escape([0xFF, 0x00, 0x48, 0x00, 0x00])
    print("Firmware lecteur :", fw.decode(errors="replace"))
    print("Firmware PN532   :", pn532(0x02).hex(" "))
    pn532(0x32, 0x05, 0xFF, 0x01, 0x01)       # 1 seule tentative de détection par poll
    beep_led(True)
    print(f"\n>>> Présente un badge sur le lecteur ({DURATION:.0f} s)...\n", flush=True)
    seen = {}
    t_end = time.time() + DURATION
    while time.time() < t_end:
        resp = pn532(0x4A, 0x01, 0x00)          # InListPassiveTarget ISO14443A
        if len(resp) >= 3 and resp[0] == 0xD5 and resp[1] == 0x4B and resp[2] >= 1:
            atqa, sak, uidlen = resp[4:6], resp[6], resp[7]
            uid = resp[8:8 + uidlen].hex(":").upper()
            if uid not in seen:
                kind = {0x08: "MIFARE Classic 1K", 0x18: "MIFARE Classic 4K",
                        0x00: "MIFARE Ultralight/NTAG", 0x20: "ISO14443-4 (DESFire, carte bancaire, ...)",
                        0x09: "MIFARE Mini", 0x28: "JCOP/Classic émulé"}.get(sak, "inconnu")
                print(f"[{time.strftime('%H:%M:%S')}] Badge détecté  UID={uid}  "
                      f"ATQA={atqa.hex()}  SAK={sak:02X} ({kind})", flush=True)
                beep_led(True)
            seen[uid] = time.time()
            pn532(0x52, 0x00)                     # InRelease
        time.sleep(0.25)
    print(f"\nFin du test : {len(seen)} badge(s) distinct(s) lu(s).")
finally:
    try:
        pn532(0x32, 0x01, 0x00)                  # coupe le champ RF
    except Exception:
        pass
    lib.libusb_release_interface(h, 0)
    if detached:
        lib.libusb_attach_kernel_driver(h, 0)
    lib.libusb_close(h)
    lib.libusb_exit(ctx)
