<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { DB, confirmAction, guard, pickFile, reader, saveFile, toast } from "../lib/app.svelte";
  import { toNaive } from "../lib/format";
  import type { Settings } from "../lib/types";

  let settings = $state<Settings | null>(null);
  let initialReader = "";
  let initialRemote = "";
  let dataPath = $state("");
  let localIp = $state<string | null>(null);

  const remoteKey = (s: Settings | null) => `${s?.remote_enabled}:${s?.remote_port}`;

  onMount(async () => {
    settings = (await guard(api.getSettings())) ?? null;
    initialReader = settings?.reader ?? "";
    initialRemote = remoteKey(settings);
    dataPath = await api.dataPath();
    localIp = (await api.localAddress()) ?? null;
  });

  async function save() {
    if (!settings) return;
    if ((await guard(api.saveSettings($state.snapshot(settings)))) !== undefined) {
      const restart = settings.reader !== initialReader || remoteKey(settings) !== initialRemote;
      toast(restart ? "Enregistré · le changement de lecteur ou de bornes s'appliquera au prochain démarrage" : "Réglages enregistrés", "success");
      // Relit les réglages : le jeton est généré côté serveur à la première activation.
      settings = (await guard(api.getSettings())) ?? settings;
    }
  }

  async function copyToken() {
    if (!settings?.remote_token) return;
    try {
      await navigator.clipboard.writeText(settings.remote_token);
      toast("Jeton copié", "success");
    } catch {
      toast("Copie impossible : sélectionnez le jeton à la main", "error");
    }
  }

  async function regenerateToken() {
    if (!settings) return;
    if (!(await confirmAction("Générer un nouveau jeton ?\n\nLes bornes déjà configurées devront recevoir le nouveau jeton."))) return;
    settings.remote_token = "";
    await save();
  }

  async function backup() {
    const p = await saveFile(`badgeuse-sauvegarde-${toNaive(new Date()).slice(0, 10)}.db`, DB);
    if (p && (await guard(api.backupDb(p))) !== undefined) toast(`Sauvegarde : ${p}`, "success", 6000);
  }

  async function restore() {
    const p = await pickFile(DB);
    if (!p) return;
    if (!(await confirmAction("Remplacer TOUTES les données actuelles par celles de cette sauvegarde ?\n\nFaites d'abord une sauvegarde si besoin."))) return;
    if ((await guard(api.restoreDb(p))) !== undefined) {
      toast("Sauvegarde restaurée", "success");
      location.hash = "#/accueil";
    }
  }
</script>

<div class="page-head"><div><h1>Réglages</h1></div></div>

<div class="stack">
  <section class="card">
    <h2>Lecteur(s) NFC</h2>
    <p>
      <span class="pill" class:present={reader.connected} class:absent={!reader.connected}>
        {reader.connected ? (reader.connected_count > 1 ? `${reader.connected_count} connectés` : "Connecté") : "Non connecté"}
      </span>
      <span class="small">{reader.message}</span>
    </p>
    {#if reader.readers.length}
      <ul class="readers small">
        {#each reader.readers as r (r.name)}
          <li>
            <span class="dot" class:on={r.connected}></span>
            <span class="mono grow">{r.name}</span>
            <span class="muted">{r.connected ? r.message : "déconnecté"}</span>
          </li>
        {/each}
      </ul>
    {/if}
    <p class="muted small">Plusieurs lecteurs peuvent être branchés en même temps (PC/SC) : ils alimentent tous le même créneau. Un lecteur branché en cours de séance est détecté automatiquement.</p>
    {#if settings}
      <div class="grid2">
        <label class="field">
          Type de lecteur
          <select bind:value={settings.reader}>
            <option value="auto">Automatique (PC/SC, multi-lecteurs ; secours USB direct)</option>
            <option value="pcsc">PC/SC uniquement (pcscd, multi-lecteurs)</option>
            <option value="usb">USB direct ACR122U (sans pcscd)</option>
            <option value="mock">Simulé (démonstration / tests)</option>
          </select>
        </label>
        <label class="row"><input type="checkbox" bind:checked={settings.reader_feedback} /> Bip et LED du lecteur (vert / orange / rouge)</label>
        <label class="field">
          Bip du lecteur
          <select bind:value={settings.reader_beep} disabled={!settings.reader_feedback}>
            <option value="normal">Normal (1 bip ; 2 si retard ; 3 si erreur)</option>
            <option value="discreet">Discret (un seul bip court)</option>
            <option value="silent">Muet (LED uniquement)</option>
          </select>
        </label>
      </div>
      <p class="muted small">La variable d'environnement <span class="mono">BADGEUSE_READER</span> est prioritaire sur ce réglage.</p>
    {/if}
    {#if !reader.connected}
      <details>
        <summary>Le lecteur n'est pas détecté ?</summary>
        <ol class="small">
          <li>Installez PC/SC : <span class="mono">sudo apt install pcscd libccid pcsc-tools</span></li>
          <li>Empêchez le pilote noyau de prendre le lecteur :<br /><span class="mono">echo "blacklist pn533_usb" | sudo tee /etc/modprobe.d/blacklist-acr122u.conf</span><br />puis débranchez / rebranchez (ou <span class="mono">sudo modprobe -r pn533_usb</span>).</li>
          <li>Testez avec <span class="mono">pcsc_scan</span>, ou sans pcscd avec <span class="mono">python3 tools/acr122u_test.py</span>.</li>
          <li>Sans pcscd, choisissez « USB direct » (l'utilisateur doit avoir accès au périphérique USB).</li>
        </ol>
      </details>
    {/if}
  </section>

  {#if settings}
    <section class="card">
      <h2>Créneaux</h2>
      <div class="grid2">
        <label class="field">Seuil de retard par défaut (minutes) <input type="number" min="0" bind:value={settings.default_late_min} /></label>
        <label class="field">Durée par défaut (minutes) <input type="number" min="5" step="5" bind:value={settings.default_duration_min} /></label>
      </div>
      <h3>Émargement automatique</h3>
      <div class="grid2">
        <label class="row"><input type="checkbox" bind:checked={settings.auto_open} /> Ouvrir les créneaux automatiquement</label>
        <label class="field">
          Ouverture (minutes avant le début)
          <input type="number" min="0" max="120" bind:value={settings.auto_open_lead_min} disabled={!settings.auto_open} />
        </label>
        <label class="row"><input type="checkbox" bind:checked={settings.auto_close} /> Clore les créneaux automatiquement à leur fin</label>
      </div>
      <p class="muted small">
        Un créneau prévu s'ouvre seul à l'approche de son début et reçoit les badges, même si l'écran d'émargement n'est pas affiché.
        La clôture automatique marque absents les étudiants attendus qui n'ont pas badgé (annulable en rouvrant le créneau).
      </p>
    </section>
    <section class="card">
      <h2>Bornes réseau</h2>
      <p class="small">
        Une borne de scan (Raspberry Pi + écran) envoie ses badges à cet ordinateur par le réseau local. Elle apparaît ci-dessus
        comme un lecteur de plus et alimente le même créneau actif ; hors ligne, elle garde les badges et les renvoie ensuite.
      </p>
      <div class="grid2">
        <label class="row"><input type="checkbox" bind:checked={settings.remote_enabled} /> Accepter les bornes réseau</label>
        <label class="field">
          Port d'écoute
          <input type="number" min="1024" max="65535" bind:value={settings.remote_port} disabled={!settings.remote_enabled} />
        </label>
      </div>
      {#if settings.remote_enabled && settings.remote_token}
        <label class="field">
          Jeton à saisir sur la borne
          <span class="row">
            <input class="mono grow" readonly value={settings.remote_token} onfocus={(e) => e.currentTarget.select()} />
            <button onclick={copyToken}>Copier</button>
            <button onclick={regenerateToken}>Régénérer</button>
          </span>
        </label>
        <p class="small">
          Adresse de la borne vers cet ordinateur :
          <span class="mono">http://{localIp ?? "<adresse-ip-du-pc>"}:{settings.remote_port}</span>
        </p>
      {/if}
      <p class="muted small">
        Réseau local uniquement : n'ouvrez jamais ce port vers Internet. Le trafic n'est pas chiffré, le jeton protège l'accès mais
        circule en clair sur le réseau : réservez un réseau de confiance (câble ou Wi-Fi protégé). Les bornes ne reçoivent que le
        prénom et l'initiale du nom. Le changement d'activation ou de port s'applique au prochain démarrage.
      </p>
    </section>
    <div><button class="primary" onclick={save}>Enregistrer les réglages</button></div>
  {/if}

  <section class="card">
    <h2>Données</h2>
    <p class="small">Toutes les données restent sur cet ordinateur, aucune n'est envoyée en ligne.</p>
    <p class="small muted">Base : <span class="mono">{dataPath}</span></p>
    <div class="row">
      <button onclick={backup}>Sauvegarder la base…</button>
      <button class="danger" onclick={restore}>Restaurer une sauvegarde…</button>
    </div>
    <p class="muted small">Données personnelles (RGPD) : conservez seulement le nécessaire. En fin d'année, exportez puis supprimez la promotion dans « Groupes ».</p>
  </section>
</div>

<style>
  .readers { list-style: none; margin: 0.25rem 0 0; padding: 0; display: flex; flex-direction: column; gap: 0.3rem; }
  .readers li { display: flex; align-items: center; gap: 0.5rem; }
  .readers .dot { width: 0.6rem; height: 0.6rem; border-radius: 50%; background: var(--absent); flex: none; }
  .readers .dot.on { background: var(--present); }
</style>
