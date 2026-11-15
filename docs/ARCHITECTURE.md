# BoothSoft — architecture

État au 11/11/2026, version 0.16. Ce document décrit ce qui **existe**. Ce qui est à
faire est dans `docs/ROADMAP.md` et `docs/specs/`.

## 1. Vue d'ensemble

Une seule application native (Rust, `eframe`/`egui` 0.32, rendu `glow` = OpenGL), un
exécutable d'environ 12 Mo, aucun service. Tout l'état vit dans la structure `Kiosk`
(`src/kiosk.rs`), modifiée uniquement par le fil d'interface. Les travaux lents sont
confiés à des fils qui répondent par des canaux.

```
                         ┌────────────────────────────────────┐
   toucher ─────────────▶│  Fil d'interface (egui)            │
                         │  Kiosk::update : lit les canaux,   │
                         │  dessine l'écran courant           │
                         └──┬───────┬─────────┬─────────┬─────┘
          commandes         │       │         │         │
                 ┌──────────▼─┐ ┌───▼─────┐ ┌─▼───────┐ ┌▼──────────────┐
                 │ bs-camera  │ │ bs-photo│ │bs-printer│ │ fils ponctuels │
                 │ lecture,   │ │ filtre, │ │ envoi au │ │ recherche des  │
                 │ décodage,  │ │ JPEG,   │ │ système  │ │ appareils,     │
                 │ mouvement  │ │ cadre   │ │          │ │ import ZIP     │
                 └────────────┘ └─────────┘ └──────────┘ └────────────────┘
```

## 2. Fils et canaux

| Fil | Fichier | Reçoit | Renvoie | Notes |
| --- | --- | --- | --- | --- |
| Interface | `src/kiosk.rs` | touchers, événements des autres fils | — | Ne bloque jamais. Se rendort tant que rien ne change. |
| `bs-camera` | `src/camera.rs` | `CameraCommand` : `Start{index}`, `Capture`, `SetPreviewEnabled`, `SetFrameRate`, `SetSleep`, `Record(Option<SyncSender<RecordFrame>>)`, `Stop`, `Shutdown` | `CameraEvent` : `Captured(RgbImage)`, `Motion`, `Status`, `Error` ; images du live par `take_preview()` | Les tampons d'image du live sont recyclés par un canal borné (3). |
| `bs-photo` | `src/photo_worker.rs` | `PhotoCommand` (image, filtre, cadre, chemins) | `PhotoEvent::Saved(PhotoResult)` ou `Failed` | File bornée à 1 travail en attente. |
| `bs-printer` | `src/printer.rs` | `PrintRequest { job_id, printer, jpeg }` | `PrintEvent::Submitted` ou `Failed` | Windows : GDI ; macOS / Linux : commande `lp`. |
| Recherche d'appareils | `src/devices.rs` | — | `DeviceInventory` (caméras, imprimantes, imprimante par défaut) | Lancé à l'ouverture de la maintenance et sur « Actualiser ». |
| `theme-import` | `src/kiosk/admin.rs` | chemin d'un ZIP | identifiant du thème ou erreur | Un seul à la fois. |
| `bs-record` | `src/guestbook.rs` | `RecordFrame` (JPEG du live, canal borné à 30) + son du micro | `RecordResult` (`Saved`, `TooShort`, `Failed`) | Vit le temps d'un message du livre d'or. Écrit l'AVI en flux. |
| `bs-playback` | `src/guestbook.rs` | fichier AVI | une image RGBA à la fois (`Player::take_frame`) | Cadencé par la position du son (`audio::Speaker`). |
| Micro (rappel `cpal`) | `src/audio.rs` | — | crête (jauge) + échantillons 48 kHz mono | Ouvert uniquement sur les écrans du livre d'or. |

Réveil de l'interface : chaque fil reçoit une fonction qui appelle
`ctx.request_repaint_after(1 ms)` (constante `WAKE`).

## 3. Écrans

`model::Screen` : `Welcome`, `Ready`, `Countdown`, `Processing`, `Review`, `Pick`,
`Thanks`, `Error`, `Admin`.

```
           5 min sans personne                    toucher / présence
   Ready ───────────────────────▶ Welcome (veille) ──────────────────▶ Ready
     │ déclencheur
     ▼
  Countdown (7 s) ── photo simple ──▶ Processing ──▶ Review ──▶ Thanks (6 s) ──▶ Ready
     │                                               │  ▲
     │                                  « Reprendre » ┘  └ « Photo suivante » (séries 2–4)
     └── rafale : N photos espacées de 0,9 s ──▶ Processing ──▶ Pick ──▶ Thanks ──▶ Ready

  Error : se relance seul toutes les 5 s.      Admin : 5 touchers en haut à gauche du live
                                               (ou en haut à droite de la veille) + code PIN.
```

- **Ready** est l'état normal : live en miroir, sélecteur « Une photo / Rafale ×N »,
  six vignettes de filtres en direct, déclencheur.
- **Welcome** n'est que l'écran de veille. La caméra passe à 4 images/s, n'affiche
  rien, et `PresenceDetector` réveille la borne.
- **Review** et **Pick** reviennent seuls au live après `photo_timeout_seconds`.
- Le rendu de chaque écran est une méthode `render_*` de `Kiosk`. `Kiosk::update` traite
  d'abord les canaux, puis dessine l'écran courant, puis programme le prochain réveil.

## 4. Le live

1. `bs-camera` lit une image (webcam : MJPEG 1280×720 à 30 img/s), la décode en RGBA
   par `mjpeg::decode` (réglages rapides), calcule une grille 16×9 de luminosité
   (`MotionGate`) et ne transmet l'image que si quelque chose a bougé (sinon 10 img/s).
2. Le fil d'interface confie l'image à `live_gl::LiveView::submit` : envoi tel quel
   dans une texture OpenGL.
3. `LiveView::paint` dessine par un `PaintCallback` : le shader applique le recadrage,
   le miroir, la matrice couleur du filtre, les coins arrondis, les vignettes et
   l'assombrissement hors zone imprimée. Aucune boucle de pixels sur le CPU.
4. Les six vignettes de filtres réutilisent la même texture.
5. Sans OpenGL utilisable : repli sur des textures egui (plus coûteux).

## 5. De la photo au tirage

1. Fin du décompte → `CameraCommand::Capture` → `CameraEvent::Captured(RgbImage)`.
2. `Kiosk::accept_capture` → `PhotoCommand` au fil photo, qui :
   applique le filtre choisi, enregistre l'original (`photo-NN.jpg`, qualité 92),
   recadre à la taille du cadre et pose le cadre PNG (`compositor::compose_owned`),
   enregistre la version imprimable (`photo-NN-cadre.jpg`, qualité 94), prépare
   l'aperçu (1600 px au plus) et un fond flou minuscule.
3. `PhotoEvent::Saved` → `storage.register_shot` → écran Review (ou vignette de rafale).
4. Impression : `Kiosk::submit_print` réserve un tirage dans `print_jobs` (quota de
   l'événement), puis envoie `PrintRequest` au fil d'impression. Un travail par exemplaire.
5. Le tirage reste « à vérifier » jusqu'à confirmation en maintenance (à automatiser :
   fiches B3 et Z3).

Le cadre : un PNG transparent de la taille du tirage (1800×1200 aujourd'hui, soit 6×4
pouces à 300 dpi). La photo est recadrée « en couverture » à cette taille, le cadre est
posé par-dessus.

## 6. Données sur disque

Racine : dossier de données local de l'utilisateur + `BoothSoft/`
(`%LOCALAPPDATA%\BoothSoft`, `~/Library/Application Support/BoothSoft`,
`$XDG_DATA_HOME/BoothSoft`).

```
BoothSoft/
├── settings.json               Réglages (model::Settings), écriture atomique
├── print-ledger.json           Quota et compteur de tirages par événement
├── print-jobs.json             Journal des tirages (à alléger : fiche B3)
├── print-exports/              Exports manuels de la maintenance
├── themes/<id>/                Thèmes importés (ZIP) — même format que les thèmes livrés
└── sessions/
    ├── <dossier-événement>/    ex. 2026-11-14-mariage-lea-hugo
    │   └── <id-séance>/
    │       ├── session.json    Manifeste (événement, photos)
    │       ├── photo-01.jpg    Original filtré
    │       └── photo-01-cadre.jpg   Version imprimée (avec cadre)
    └── <id-séance>/            Anciennes séances (avant les événements), toujours lues
```

### Réglages (`model::Settings`)

`event_name` (événement par défaut), `events` (planning : nom, date, début, fin),
`theme_id`, `frame_path`, `camera_index`, `demo_mode`, `shots_per_session` (1–4, mode
photo), `burst_enabled`, `burst_count` (3–10), `photo_timeout_seconds`, `session_limit`,
`print_enabled`, `printer_name` (vide = imprimante par défaut du système), `max_copies`,
`live_fps`, `sleep_minutes`, `wake_on_motion`, `fullscreen`, `admin_pin_hash` (bcrypt),
`defaults_version` (migrations de valeurs par défaut : `Settings::migrate`).

Tout nouveau champ doit avoir une valeur par défaut (`#[serde(default)]` sur la
structure) pour que les anciens fichiers restent lisibles.

### Événement en cours

`Settings::current_event(maintenant)` renvoie le créneau du planning qui contient
l'heure courante (fin ≤ début = se termine le lendemain), sinon l'événement par défaut.
Le dossier, le quota de tirages et la limite de séances en dépendent.

## 7. Thèmes

> **État transitoire.** Aujourd'hui le cadre des tirages est un fichier du thème, et
> deux thèmes livrés (`mariage`, `anniversaire`) sont en réalité des cadres. La cible :
> le thème est l'habillage de l'entreprise, le cadre appartient à l'événement
> (`docs/specs/EVENT_FRAMES.md`, `docs/specs/MAINTENANCE_PATTERNS.md` § 1).

Un thème est un dossier :

```
themes/<id>/
├── config.toml
└── assets/
    ├── logo.png         Transparent, libre de proportions
    ├── frame.png        Cadre d'impression, 1800×1200, transparent là où la photo apparaît
    └── background.png   Facultatif : illustration de l'écran de veille (1600×1000)
```

`config.toml` (schéma 1, champs inconnus refusés) : `name`, `[colors]` (`background`,
`foreground`, `primary`, `accent`, `surface`, `muted`, au format `#RRGGBB`), `[assets]`
(`logo`, `frame`, `background` facultatif), `[ui]` (libellés : `shutter_label`,
`retake_label`, `finish_label` obligatoires ; `print_label`, `start_label`,
`welcome_eyebrow`, `welcome_title`, `welcome_subtitle`, `thanks_title`,
`thanks_subtitle` facultatifs).

- Thèmes livrés : dossier `themes/` à côté de l'exécutable (lecture seule une fois installé).
- Thèmes de l'utilisateur : `BoothSoft/themes/`. Un thème livré a priorité sur un thème
  utilisateur de même identifiant (`theme_config::load_any`).
- Validation stricte dans `theme_config::load` : identifiant en minuscules ASCII, pas de
  lien symbolique, PNG seulement, 16 Mo et 4096 px au plus, textes bornés.
- Thème actif : `theme::active()` (un `Arc` remplacé par `theme::replace_active`).
  Les couleurs sont lues par `theme::primary()` etc. et par `ui::Palette::current()`.
- La maintenance s'habille aussi aux couleurs du thème (`admin.rs::Pal`).

## 8. Maintenance

`src/kiosk/admin.rs`. Barre du haut, menu latéral (`AdminSection` : Vue d'ensemble,
Événements, Impression, Caméra & écran, Thème, Sécurité, Données), page en cartes,
barre « Modifications non enregistrées » tant que `Settings` diffère de la copie
enregistrée (`Kiosk::admin_saved`). Le thème et le code PIN s'enregistrent
immédiatement (`save_only`), le reste au bouton Enregistrer ou en quittant.

Sortie de la maintenance : `request_leave(PendingLeave)` exécute l'action tout de suite
si `unsaved()` est vide, sinon ouvre la fenêtre « Quitter sans enregistrer ? »
(`render_leave_dialog`, couche `egui::Order::Foreground` qui bloque tout le reste).
`unsaved()` liste « Planning des événements », « Réglages », « Tirages prévus ».
Suppression d'un événement : `pending_event_delete` + `render_delete_event_dialog`.

## 8 bis. Gardien et journal

- `boothsoft-guard` (`src/bin/boothsoft-guard.rs`) lance `boothsoft` avec
  `BOOTHSOFT_GUARDED=1` et lit sa sortie standard. `src/watchdog.rs` : le fil
  `bs-heartbeat` réveille l'interface toutes les 5 s et n'écrit `BOOTHSOFT-HEARTBEAT`
  que si l'interface a dessiné depuis (`UiPulse::stamp` dans `Kiosk::update`).
  30 s sans battement → arrêt forcé et relance. Code de sortie 0 = fermeture voulue
  (le gardien s'arrête), 75 = redémarrage demandé, autre = plantage.
- `src/log.rs` : journal du jour dans `BoothSoft/logs/`, écrit par le fil `bs-log`.
  `boothsoft::log::info(...)` depuis n'importe quel fil, jamais bloquant.

Composants réutilisables du fichier : `card`, `row`, `divider`, `toggle`, `stepper`,
`segmented`, `date_stepper`, `time_stepper`, `progress`, `pill`, `btn`.

## 9. Mesure

`scripts/perf/monitor.py` lance l'application sur Xvfb, rejoue des clics, relève le CPU
(moyenne par étape), la RAM, le détail par fil, et prend des captures. Les résultats de
référence sont dans `docs/GUEST_UI.md`. `src/bin/boothsoft-bench.rs` mesure la
composition seule.
