#!/usr/bin/env python3
"""Testeur de carte NFC pour l'ACR122U, via PC/SC (pcscd).

Sans dépendance Python : appelle directement libpcsclite par ctypes.
Affiche le lecteur, l'UID, l'ATR et le contenu de la carte, et permet
d'écrire (éditer) une carte de stockage MIFARE Classic / Ultralight / NTAG.

Exemples :
    python3 tools/carte_tool.py                 # infos lecteur + attente d'une carte + dump
    python3 tools/carte_tool.py info            # idem (par défaut)
    python3 tools/carte_tool.py read 4          # lit le bloc/la page 4
    python3 tools/carte_tool.py write 4 00112233 # écrit 4 octets (Ultralight) ou 16 (Classic)
    python3 tools/carte_tool.py apdu FF CA 00 00 00   # envoie une APDU brute

Édition : seules les cartes de stockage (MIFARE Classic, Ultralight, NTAG)
sont modifiables simplement. Une carte DESFire / bancaire (SAK 0x20) est
protégée par des clés : le contenu n'est que listé, pas réécrit.
"""
import ctypes as C
import sys
import time

# --- liaison libpcsclite (ABI Linux : DWORD / handles = unsigned long) ------
DWORD = C.c_ulong
LONG = C.c_long

SCARD_SCOPE_USER = 0
SCARD_SHARE_SHARED = 2
SCARD_SHARE_DIRECT = 3
SCARD_PROTOCOL_T0 = 1
SCARD_PROTOCOL_T1 = 2
SCARD_PROTOCOL_ANY = SCARD_PROTOCOL_T0 | SCARD_PROTOCOL_T1
SCARD_LEAVE_CARD = 0
SCARD_RESET_CARD = 1
SCARD_S_SUCCESS = 0


class SCARD_IO_REQUEST(C.Structure):
    _fields_ = [("dwProtocol", C.c_ulong), ("cbPciLength", C.c_ulong)]


pcsc = C.CDLL("libpcsclite.so.1")


def _err(name, rv):
    raise RuntimeError(f"{name} a échoué : 0x{rv & 0xFFFFFFFF:08X}")


def establish():
    ctx = LONG()
    rv = pcsc.SCardEstablishContext(DWORD(SCARD_SCOPE_USER), None, None, C.byref(ctx))
    if rv != SCARD_S_SUCCESS:
        _err("SCardEstablishContext", rv)
    return ctx


def list_readers(ctx):
    length = DWORD(0)
    rv = pcsc.SCardListReaders(ctx, None, None, C.byref(length))
    if rv != SCARD_S_SUCCESS or length.value == 0:
        return []
    buf = C.create_string_buffer(length.value)
    rv = pcsc.SCardListReaders(ctx, None, buf, C.byref(length))
    if rv != SCARD_S_SUCCESS:
        _err("SCardListReaders", rv)
    raw = buf.raw[: length.value]
    return [r.decode(errors="replace") for r in raw.split(b"\x00") if r]


def connect(ctx, reader, direct=False):
    hcard = LONG()
    proto = DWORD()
    mode = SCARD_SHARE_DIRECT if direct else SCARD_SHARE_SHARED
    pref = 0 if direct else SCARD_PROTOCOL_ANY
    rv = pcsc.SCardConnect(ctx, reader.encode(), DWORD(mode), DWORD(pref),
                           C.byref(hcard), C.byref(proto))
    if rv != SCARD_S_SUCCESS:
        _err("SCardConnect", rv)
    return hcard, proto.value


def get_atr(hcard):
    rlen = DWORD(0)
    atrlen = DWORD(33)
    atr = C.create_string_buffer(33)
    state = DWORD(0)
    proto = DWORD(0)
    rv = pcsc.SCardStatus(hcard, None, C.byref(rlen), C.byref(state),
                          C.byref(proto), atr, C.byref(atrlen))
    if rv != SCARD_S_SUCCESS:
        return b""
    return atr.raw[: atrlen.value]


def transmit(hcard, proto, apdu):
    send_pci = SCARD_IO_REQUEST.in_dll(
        pcsc, "g_rgSCardT1Pci" if proto == SCARD_PROTOCOL_T1 else "g_rgSCardT0Pci")
    data = bytes(apdu)
    sbuf = C.create_string_buffer(data, len(data))
    rbuf = C.create_string_buffer(258)
    rlen = DWORD(258)
    rv = pcsc.SCardTransmit(hcard, C.byref(send_pci), sbuf, DWORD(len(data)),
                            None, rbuf, C.byref(rlen))
    if rv != SCARD_S_SUCCESS:
        _err("SCardTransmit", rv)
    return rbuf.raw[: rlen.value]


def control(hcard, code, payload=b""):
    rbuf = C.create_string_buffer(258)
    rlen = DWORD(0)
    sbuf = C.create_string_buffer(bytes(payload), len(payload)) if payload else None
    rv = pcsc.SCardControl(hcard, DWORD(code), sbuf, DWORD(len(payload)),
                           rbuf, DWORD(258), C.byref(rlen))
    if rv != SCARD_S_SUCCESS:
        return b""
    return rbuf.raw[: rlen.value]


def disconnect(hcard):
    pcsc.SCardDisconnect(hcard, DWORD(SCARD_LEAVE_CARD))


def release(ctx):
    pcsc.SCardReleaseContext(ctx)


# --- aides APDU -------------------------------------------------------------
def hx(b):
    return " ".join(f"{x:02X}" for x in b)


def status(resp):
    return resp[-2:] if len(resp) >= 2 else b"\x00\x00"


def ok(resp):
    return status(resp) == b"\x90\x00"


def pcsc_escape_code():
    # IOCTL_CCID_ESCAPE = SCARD_CTL_CODE(1) ; sur pcsc-lite : 0x42000000 + 1
    return 0x42000000 + 1


SAK_KINDS = {
    0x00: "MIFARE Ultralight / NTAG",
    0x08: "MIFARE Classic 1K",
    0x09: "MIFARE Mini",
    0x18: "MIFARE Classic 4K",
    0x20: "ISO 14443-4 (DESFire, carte bancaire, passeport…)",
    0x28: "JCOP / Classic émulé",
}

CLASSIC_KEYS = [
    bytes.fromhex("FFFFFFFFFFFF"),
    bytes.fromhex("A0A1A2A3A4A5"),
    bytes.fromhex("D3F7D3F7D3F7"),
    bytes.fromhex("000000000000"),
]


def read_uid(hcard, proto):
    r = transmit(hcard, proto, [0xFF, 0xCA, 0x00, 0x00, 0x00])
    return r[:-2] if ok(r) else b""


def guess_from_atr(atr):
    """ATR 3B 8F 80 01 ... renvoie (nom, sak) pour les cartes sans contact."""
    sak = None
    name = "inconnu"
    # ATR « storage card » PC/SC : ... 4F 0C A0 00 00 03 06 SS ... (SS = type)
    idx = atr.find(bytes([0x4F, 0x0C, 0xA0, 0x00, 0x00, 0x03, 0x06]))
    if idx != -1 and len(atr) >= idx + 11:
        std = atr[idx + 8]         # type de carte selon PC/SC
        name = {0x00: "carte 14443-4 (DESFire, bancaire…)",
                0x01: "MIFARE Classic 1K", 0x02: "MIFARE Classic 4K",
                0x03: "MIFARE Ultralight", 0x26: "MIFARE Mini",
                0x3A: "MIFARE Ultralight C"}.get(std, f"code 0x{std:02X}")
    elif atr and atr[0] == 0x3B:
        # ATR sans signature « storage card » → carte à puce ISO 14443-4
        name = "ISO 14443-4 (DESFire / carte sans contact)"
    return name, sak


# --- dumps par type ---------------------------------------------------------
def dump_classic(hcard, proto, blocks=64):
    print("\nContenu MIFARE Classic (lecture avec les clés par défaut) :")
    any_read = False
    for sector in range(blocks // 4):
        authed = False
        for key in CLASSIC_KEYS:
            transmit(hcard, proto, [0xFF, 0x82, 0x00, 0x00, 0x06, *key])
            trailer = sector * 4 + 3
            auth = transmit(hcard, proto,
                            [0xFF, 0x86, 0x00, 0x00, 0x05, 0x01, 0x00, trailer, 0x60, 0x00])
            if ok(auth):
                authed = True
                used = key
                break
        if not authed:
            print(f"  Secteur {sector:2d} : clé par défaut refusée (lecture impossible)")
            continue
        for b in range(sector * 4, sector * 4 + 4):
            r = transmit(hcard, proto, [0xFF, 0xB0, 0x00, b, 0x10])
            if ok(r):
                any_read = True
                tag = "  (trailer)" if b % 4 == 3 else ""
                print(f"  Bloc {b:2d} : {hx(r[:-2])}{tag}")
            else:
                print(f"  Bloc {b:2d} : lecture refusée ({hx(status(r))})")
        print(f"           clé A utilisée : {used.hex().upper()}")
    if not any_read:
        print("  Aucun bloc lisible avec les clés usuelles (carte personnalisée).")


def dump_ultralight(hcard, proto, pages=44):
    print("\nContenu MIFARE Ultralight / NTAG (4 octets par page) :")
    for p in range(0, pages, 4):
        r = transmit(hcard, proto, [0xFF, 0xB0, 0x00, p, 0x10])  # lit 4 pages d'un coup
        if not ok(r):
            break
        data = r[:-2]
        for j in range(0, len(data), 4):
            print(f"  Page {p + j // 4:2d} : {hx(data[j:j + 4])}")


def desfire(hcard, proto, cmd, data=b""):
    apdu = [0x90, cmd, 0x00, 0x00, len(data), *data, 0x00]
    return transmit(hcard, proto, apdu)


def dump_desfire(hcard, proto):
    print("\nCarte ISO 14443-4 (type DESFire). Lecture de la structure :")
    v = desfire(hcard, proto, 0x60)  # GetVersion
    if len(v) >= 9:
        print(f"  GetVersion : {hx(v[:-2])}  (fabricant/HW dans les premiers octets)")
    aids = desfire(hcard, proto, 0x6A)  # GetApplicationIDs
    if ok(aids) or status(aids)[0] == 0x91:
        ids = aids[:-2]
        if ids:
            trip = [ids[i:i + 3] for i in range(0, len(ids), 3)]
            print(f"  Applications (AID) : {', '.join(a[::-1].hex().upper() for a in trip)}")
        else:
            print("  Aucune application applicative accessible sans authentification.")
    else:
        print(f"  GetApplicationIDs : {hx(status(aids))} (accès protégé par clé)")
    print("  → Le contenu applicatif d'une DESFire est chiffré/protégé par des clés ;")
    print("    il ne peut pas être lu ni édité sans les clés de l'émetteur de la carte.")


# --- commandes --------------------------------------------------------------
def with_card(fn, direct=False, wait=True, timeout=30):
    ctx = establish()
    try:
        readers = list_readers(ctx)
        if not readers:
            print("Aucun lecteur PC/SC. pcscd tourne-t-il ? (sudo systemctl start pcscd)")
            return 2
        reader = readers[0]
        print(f"Lecteur : {reader}")
        # infos lecteur via mode direct (firmware)
        try:
            h0, _ = connect(ctx, reader, direct=True)
            fw = control(h0, pcsc_escape_code(), [0xFF, 0x00, 0x48, 0x00, 0x00])
            if fw:
                print(f"Firmware : {fw.decode(errors='replace')}")
            disconnect(h0)
        except Exception:
            pass

        deadline = time.time() + timeout
        while True:
            try:
                hcard, proto = connect(ctx, reader)
                break
            except RuntimeError:
                if not wait or time.time() > deadline:
                    print("Aucune carte présentée dans le délai.")
                    return 1
                print("  … présente une carte sur le lecteur", end="\r", flush=True)
                time.sleep(0.4)
        try:
            return fn(hcard, proto)
        finally:
            disconnect(hcard)
    finally:
        release(ctx)


def cmd_info(args):
    def body(hcard, proto):
        atr = get_atr(hcard)
        uid = read_uid(hcard, proto)
        name, _ = guess_from_atr(atr)
        print("\n--- Carte détectée ---")
        print(f"ATR : {hx(atr)}")
        if uid:
            print(f"UID : {uid.hex(':').upper()}  ({len(uid)} octets)")
            if len(uid) == 4 and uid[0] == 0x08:
                print("      ⚠ UID aléatoire (non stable) : inutilisable pour l'émargement.")
        else:
            print("UID : non lu")
        print(f"Type (d'après l'ATR) : {name}")

        low = name.lower()
        if "classic 1k" in low or "mini" in low:
            dump_classic(hcard, proto, 64)
        elif "classic 4k" in low:
            dump_classic(hcard, proto, 256)
        elif "ultralight" in low or "ntag" in low:
            dump_ultralight(hcard, proto)
        elif "14443-4" in low or "desfire" in low or "bancaire" in low:
            dump_desfire(hcard, proto)
        else:
            dump_desfire(hcard, proto)
        return 0
    return with_card(body)


def cmd_read(args):
    if not args:
        print("Usage : read <bloc/page>")
        return 2
    block = int(args[0], 0)

    def body(hcard, proto):
        atr = get_atr(hcard)
        name, _ = guess_from_atr(atr)
        if "classic" in name.lower():
            for key in CLASSIC_KEYS:
                transmit(hcard, proto, [0xFF, 0x82, 0x00, 0x00, 0x06, *key])
                if ok(transmit(hcard, proto,
                               [0xFF, 0x86, 0x00, 0x00, 0x05, 0x01, 0x00, block, 0x60, 0x00])):
                    break
        r = transmit(hcard, proto, [0xFF, 0xB0, 0x00, block, 0x10])
        if ok(r):
            print(f"Bloc/page {block} : {hx(r[:-2])}")
            return 0
        print(f"Lecture refusée : {hx(status(r))}")
        return 1
    return with_card(body)


def cmd_write(args):
    if len(args) < 2:
        print("Usage : write <bloc/page> <octets en hexa>")
        print("  Ultralight/NTAG : 4 octets (ex. write 6 DEADBEEF)")
        print("  MIFARE Classic  : 16 octets (ex. write 4 00112233...).")
        return 2
    block = int(args[0], 0)
    data = bytes.fromhex("".join(args[1:]).replace(":", ""))
    if len(data) not in (4, 16):
        print(f"Longueur {len(data)} invalide : attendu 4 (Ultralight) ou 16 (Classic).")
        return 2

    def body(hcard, proto):
        atr = get_atr(hcard)
        name, _ = guess_from_atr(atr)
        low = name.lower()
        if "desfire" in low or "14443-4" in low or "bancaire" in low:
            print("Carte DESFire / bancaire : écriture impossible sans les clés de l'émetteur.")
            return 1
        if "classic" in low:
            if len(data) != 16:
                print("MIFARE Classic : 16 octets requis.")
                return 2
            if block % 4 == 3:
                print(f"Bloc {block} = trailer de secteur (clés/droits d'accès).")
                print("Refus par sécurité : l'écrire peut rendre le secteur illisible.")
                return 1
            for key in CLASSIC_KEYS:
                transmit(hcard, proto, [0xFF, 0x82, 0x00, 0x00, 0x06, *key])
                if ok(transmit(hcard, proto,
                               [0xFF, 0x86, 0x00, 0x00, 0x05, 0x01, 0x00, block, 0x60, 0x00])):
                    break
            cur = transmit(hcard, proto, [0xFF, 0xB0, 0x00, block, 0x10])
            if ok(cur):
                print(f"Avant : {hx(cur[:-2])}")
            r = transmit(hcard, proto, [0xFF, 0xD6, 0x00, block, 0x10, *data])
        else:  # Ultralight / NTAG : 4 octets par page
            if len(data) != 4:
                print("Ultralight / NTAG : 4 octets requis.")
                return 2
            cur = transmit(hcard, proto, [0xFF, 0xB0, 0x00, block, 0x04])
            if ok(cur):
                print(f"Avant (page {block}) : {hx(cur[:4])}")
            r = transmit(hcard, proto, [0xFF, 0xD6, 0x00, block, 0x04, *data])
        if ok(r):
            print(f"Écrit sur le bloc/page {block} : {hx(data)}")
            return 0
        print(f"Écriture refusée : {hx(status(r))}")
        return 1
    return with_card(body)


def cmd_apdu(args):
    if not args:
        print("Usage : apdu FF CA 00 00 00")
        return 2
    apdu = bytes(int(x, 16) for x in "".join(a if " " in a else a + " " for a in args).split())

    def body(hcard, proto):
        r = transmit(hcard, proto, apdu)
        print(f"← {hx(r)}   (statut {hx(status(r))})")
        return 0 if ok(r) else 1
    return with_card(body)


def main():
    cmd = sys.argv[1] if len(sys.argv) > 1 else "info"
    args = sys.argv[2:]
    table = {"info": cmd_info, "read": cmd_read, "write": cmd_write, "apdu": cmd_apdu}
    fn = table.get(cmd)
    if not fn:
        print(__doc__)
        return 2
    try:
        return fn(args)
    except RuntimeError as e:
        print(f"Erreur : {e}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
