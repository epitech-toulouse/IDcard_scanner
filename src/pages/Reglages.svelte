<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { DB, appUpdate, confirmAction, guard, installAppUpdate, pickFile, reader, saveFile, toast } from "../lib/app.svelte";
  import { toNaive } from "../lib/format";
  import type { Settings, UpdateInfo } from "../lib/types";

  let settings = $state<Settings | null>(null);
  let initialReader = "";
  let initialRemote = "";
  let dataPath = $state("");
  let localIp = $state<string | null>(null);

  let upd = $state<UpdateInfo>({ public_key: null, offer: null, bundled: null });
  // Le paquet embarqué est déjà celui proposé aux bornes : rien à faire.
  const upToDate = $derived(upd.bundled !== null && upd.offer?.version === upd.bundled);
  const DEB = [{ name: "Paquet de la borne", extensions: ["deb"] }];

  const remoteKey = (s: Settings | null) => `${s?.remote_enabled}:${s?.remote_port}`;

  onMount(async () => {
    settings = (await guard(api.getSettings())) ?? null;
    initialReader = settings?.reader ?? "";
    initialRemote = remoteKey(settings);
    dataPath = await api.dataPath();
    localIp = (await api.localAddress()) ?? null;
    upd = (await guard(api.updateInfo())) ?? upd;
  });

  async function createKeys() {
    upd = (await guard(api.updateGenerateKeys(false))) ?? upd;
  }

  async function regenerateKeys() {
    if (!(await confirmAction("Générer une nouvelle clé de signature ?\n\nLa mise à jour publiée est retirée et chaque borne devra recevoir la nouvelle clé publique (update_public_key), sinon elle refusera toute mise à jour."))) return;
    upd = (await guard(api.updateGenerateKeys(true))) ?? upd;
  }

  async function publishUpdate() {
    const p = await pickFile(DEB);
    if (!p) return;
    const next = await guard(api.updatePublish(p));
    if (next) {
      upd = next;
      toast(`Mise à jour ${next.offer?.version} proposée aux bornes : elles l'installent quand plus personne ne badge`, "success", 7000);
    }
  }

  let checking = $state(false);
  async function checkAppUpdate() {
    checking = true;
    try {
      const u = await guard(api.appUpdateCheck());
      if (u === null) toast("Badgeuse est à jour", "success");
      else if (u) {
        appUpdate.info = u;
        appUpdate.dismissed = false;
      }
    } finally {
      checking = false;
    }
  }

  async function updateKiosks() {
    const next = await guard(api.updatePublishBundled());
    if (next) {
      upd = next;
      toast(`Version ${next.offer?.version} envoyée aux bornes : chacune l'installe dès que plus personne ne badge (environ une minute)`, "success", 7000);
    }
  }

  async function withdrawUpdate() {
    upd = (await guard(api.updateUnpublish())) ?? upd;
  }

  async function copyKey() {
    if (!upd.public_key) return;
    try {
      await navigator.clipboard.writeText(upd.public_key);
      toast("Clé publique copiée", "success");
    } catch {
      toast("Copie impossible : sélectionnez la clé à la main", "error");
    }
  }

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
    <section class="card">
      <h2>Mise à jour des bornes</h2>
      <p class="small">
        Un clic suffit : l'appli contient le paquet de la borne de sa propre version. Elle le signe et les bornes l'installent
        d'elles-mêmes dès que personne n'a badgé depuis une minute et que leur file est vide. Une borne n'accepte que les paquets
        signés avec la clé de cet ordinateur.
      </p>
      {#if !upd.public_key}
        <p class="small">Avant la première mise à jour, créez la clé et copiez-la une fois sur chaque borne.</p>
        <button onclick={createKeys}>Créer la clé de signature</button>
      {:else}
        <div class="row">
          {#if upd.bundled}
            <button class="primary" onclick={updateKiosks} disabled={!settings.remote_enabled || upToDate}>
              {upToDate ? `Bornes en version ${upd.bundled}` : `Mettre à jour les bornes (version ${upd.bundled})`}
            </button>
          {:else}
            <span class="muted small">Pas de paquet de borne dans cette appli (version de développement).</span>
          {/if}
          {#if upd.offer}<button onclick={withdrawUpdate}>Retirer</button>{/if}
        </div>
        {#if !settings.remote_enabled}<p class="muted small">Activez d'abord les bornes réseau ci-dessus.</p>{/if}
        <p class="small">
          {#if upd.offer}
            Proposée aux bornes : <strong>version {upd.offer.version}</strong> ({(upd.offer.size / 1048576).toFixed(1)} Mo).
          {:else}
            Aucune mise à jour proposée.
          {/if}
        </p>
        <details>
          <summary class="small">Avancé</summary>
          <label class="field">
            Clé publique, à copier dans <span class="mono">update_public_key</span> de <span class="mono">/etc/badgeuse-kiosk.toml</span> (une fois par borne)
            <span class="row">
              <input class="mono grow" readonly value={upd.public_key} onfocus={(e) => e.currentTarget.select()} />
              <button onclick={copyKey}>Copier</button>
              <button onclick={regenerateKeys}>Régénérer</button>
            </span>
          </label>
          <div class="row">
            <button onclick={publishUpdate} disabled={!settings.remote_enabled}>Publier un autre paquet…</button>
            <span class="muted small">fichier <span class="mono">badgeuse-kiosk_&lt;version&gt;_arm64.deb</span> d'une release</span>
          </div>
        </details>
      {/if}
      <p class="muted small">
        La version installée de chaque borne s'affiche dans la liste des lecteurs. Une borne n'installe jamais une version plus
        ancienne que la sienne ; la clé privée ne quitte jamais cet ordinateur.
      </p>
    </section>
    <section class="card">
      <h2>Mise à jour de Badgeuse</h2>
      <label class="row"><input type="checkbox" bind:checked={settings.app_update_check} /> Chercher une nouvelle version au démarrage puis chaque jour</label>
      <p class="muted small">
        Seul accès à Internet de l'appli : la lecture de la dernière version publiée sur GitHub. Aucune donnée n'est envoyée.
        Le paquet est vérifié (signature) avant d'être installé ; l'appli redémarre ensuite.
      </p>
      <div class="row">
        <button onclick={checkAppUpdate} disabled={checking || appUpdate.installing}>{checking ? "Recherche…" : "Vérifier maintenant"}</button>
        {#if appUpdate.info}
          <button class="primary" onclick={installAppUpdate} disabled={appUpdate.installing}>
            {appUpdate.installing ? "Installation…" : `Installer la version ${appUpdate.info.version}`}
          </button>
        {/if}
      </div>
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
