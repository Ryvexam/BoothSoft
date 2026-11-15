# Spécification — Cadres des événements (chantier F1)

Statut : **à développer**. Lis d'abord `AGENTS.md`, `docs/ARCHITECTURE.md` § 5 à § 7 et
`docs/specs/MAINTENANCE_PATTERNS.md` (vocabulaire, confirmation avant de quitter,
historique) : ce document s'appuie dessus et ne les répète pas.

## 1. Le principe

**Le cadre appartient à l'événement, pas au thème.**

- Le *thème* habille la borne aux couleurs de l'entreprise qui l'exploite. Il ne change
  pas d'un événement à l'autre.
- Chaque *événement* du planning a **son propre cadre** : celui qui sera posé sur les
  tirages 10×15 de cette prestation (prénoms, date, logo du client, style de la fête).
- La borne applique le bon cadre toute seule à l'heure de l'événement, comme elle
  applique déjà son dossier de photos et son quota de tirages.
- La photo remplit **toute** la surface du tirage ; le cadre est posé par-dessus
  (comportement actuel de `compositor::compose_owned`, **à conserver tel quel**).

## 2. Ce qui existe et ce qui change

| Aujourd'hui | Demain |
| --- | --- |
| Le cadre est un fichier du thème (`themes/<id>/assets/frame.png`). Changer de cadre = changer de thème. | Le cadre est un objet à part, rangé dans une **bibliothèque de cadres**. Un événement en désigne un. |
| `Settings.frame_path` = chemin du cadre du thème actif, écrit à l'application du thème (`admin.rs::apply_selected_theme`, `kiosk.rs::new`). | Le cadre effectif est **calculé** à partir de l'événement en cours (§ 4.4). `frame_path` n'est plus écrit (champ gardé pour lire les anciens fichiers, ignoré). |
| Thèmes livrés `mariage` et `anniversaire` : en réalité des cadres déguisés en thèmes. | Supprimés des thèmes. Leurs cadres deviennent des **modèles livrés** de la bibliothèque. Seul thème livré restant : `default`, renommé « Party » (générique festif). Un thème de marque est privé : c'est un thème d'utilisateur (fiche O3 de la feuille de route). |
| `EventSlot` : nom, date, début, fin. | `EventSlot` gagne `frame` (identifiant de cadre) et les textes du cadre (§ 4.2). |

Migration des installations existantes : voir § 9.

## 3. Décisions de conception

| # | Décision | Raison |
| --- | --- | --- |
| D1 | Un **cadre** = un dossier `frames/<id>/` contenant `frame.toml` (nom, type, paramètres) et, pour un cadre importé, `frame.png`. | Même logique que les thèmes : un dossier validé, jamais un chemin libre. |
| D2 | Deux types de cadre : **image** (PNG transparent importé, figé) et **modèle** (fabriqué dans l'application à partir de paramètres : bordure, bandeau, polaroïd, avec logo et deux lignes de texte). | Couvrir « j'ai un cadre fait par un graphiste » et « je le fais en deux minutes sur la borne ». |
| D3 | Un cadre *modèle* contient des **variables** (`{event}`, `{date}`, `{ligne1}`, `{ligne2}`) remplies par chaque événement. Le même modèle sert à tous les événements ; chacun obtient **son** cadre, à son nom et à sa date. | « Chaque événement a son propre cadre » sans refaire un cadre à la main à chaque fois. |
| D4 | Le PNG réellement imprimé est **rendu une fois** par événement et mis en cache sur le disque. Jamais de rendu de cadre pendant une prise de vue. | Performances, et un tirage ne dépend jamais d'un calcul à la volée. |
| D5 | Ordre de choix du cadre : cadre de l'événement en cours → cadre par défaut (réglage général) → cadre de secours du thème (`assets/frame.png`) → aucun cadre. | Il y a toujours un tirage possible. |
| D6 | Le cadre ne change **jamais pendant une séance** : il est fixé au début de la séance (`start_session`) et vaut jusqu'à sa fin, même si l'heure de fin de l'événement tombe au milieu. | Cohérence d'une rafale. |
| D7 | Taille unique `compositor::PRINT_SIZE = (1800, 1200)` (10×15 à 300 dpi). Tout cadre est ramené à cette taille. | Un seul format. Le jour où la taille exacte de la DNP QW410 est connue, on change la constante et on vide le cache. |
| D8 | Rendu, import, encodage PNG : toujours dans un fil de travail. | Règle « rien de lent sur le fil d'interface ». |
| D9 | Enregistrement atomique, 3 versions d'historique, confirmation avant de quitter : règles de `MAINTENANCE_PATTERNS.md`. | Cohérence de la maintenance. |
| D11 | Les cadres livrés sont **génériques** : aucun nom ni logo de marque. Le cadre « Bandeau » affiche le logo du **thème actif** (`logo_source = "theme"`) : chaque exploitant y voit le sien sans rien régler. | Projet open source ; la marque de l'exploitant vient de son thème. |
| D10 | Aucune nouvelle dépendance : `image` (PNG, JPEG), `ab_glyph` (texte), `fast_image_resize` sont déjà là. | Taille et RAM. |

## 4. Modèle de données

### 4.1 Bibliothèque de cadres

```
<installation>/frames/            Cadres livrés (lecture seule)
├── sans-cadre/frame.toml
├── bandeau/frame.toml
├── polaroid/frame.toml
├── mariage/{frame.toml, frame.png}
└── anniversaire/{frame.toml, frame.png}

BoothSoft/frames/                 Cadres de l'utilisateur (créés, importés, copies modifiées)
└── <id>/
    ├── frame.toml
    ├── frame.png                 (type image seulement)
    └── logo.png                  (facultatif : logo propre à ce cadre, sinon logo du thème)

BoothSoft/frames-cache/           PNG rendus, un par (cadre, événement)
└── <id-cadre>--<dossier-événement>--<empreinte>.png
```

Identifiant de cadre : mêmes règles qu'un identifiant de thème
(`theme_config::valid_theme_id`). Un cadre livré a priorité sur un cadre utilisateur de
même identifiant ; « Modifier » un cadre livré crée une copie utilisateur
(`<id>-perso`), comme pour les thèmes.

### 4.2 `frame.toml`

```toml
schema_version = 1
name = "Bandeau"
kind = "template"                 # "image" | "template"

# --- kind = "image" : rien d'autre, le fichier frame.png fait foi ---

# --- kind = "template" ---
[template]
style = "band"                    # "none" | "border" | "band" | "polaroid"
color = "#FFFFFF"                 # couleur du cadre
text_color = "#263A2C"
border = 36                       # bordure en px sur 1800×1200 (0–120)
band = 190                        # hauteur du bandeau bas en px (120–360) ; ignoré pour border/none
radius = 0                        # arrondi de la fenêtre photo en px (0–80)
logo = "left"                     # "none" | "left" | "center" | "right"
logo_source = "theme"             # "theme" (logo du thème actif) | "frame" (logo.png du cadre)
line1 = "{ligne1}"                # texte principal, 60 caractères au plus
line2 = "{ligne2}"                # texte secondaire, 60 caractères au plus
```

Variables utilisables dans `line1` et `line2` :

| Variable | Valeur |
| --- | --- |
| `{event}` | Nom de l'événement |
| `{date}` | Date de l'événement, « 14 novembre 2026 » (mois en français, tableau fixe) |
| `{ligne1}` | Texte libre saisi sur l'événement (ex. « Léa & Hugo ») ; vide → nom de l'événement |
| `{ligne2}` | Second texte libre de l'événement ; vide → date de l'événement |

Par défaut un modèle utilise `{ligne1}` et `{ligne2}` : sans rien saisir, le tirage
porte le nom et la date de l'événement ; en saisissant deux textes, on personnalise.

### 4.3 Événement

Ajouts à `model::EventSlot` (tous avec valeur par défaut, pour relire les anciens fichiers) :

```rust
pub struct EventSlot {
    pub name: String, pub date: String, pub start: String, pub end: String,   // existants
    /// Identifiant du cadre de cet événement ; vide = cadre par défaut.
    pub frame: String,
    /// Textes du cadre pour cet événement ({ligne1}, {ligne2}) ; vides = nom et date.
    pub frame_line1: String,
    pub frame_line2: String,
}
```

Ajouts à `model::Settings` :

```rust
/// Cadre utilisé hors planning et pour les événements qui n'en ont pas choisi.
pub default_frame: String,          // défaut : "bandeau"
pub default_frame_line1: String,    // défaut : "" (→ nom de l'événement par défaut)
pub default_frame_line2: String,    // défaut : "" (→ date du jour)
```

`normalized()` : textes coupés à 60 caractères, caractères de contrôle retirés,
identifiant invalide → vide.

### 4.4 Cadre effectif

Nouvelle fonction (bibliothèque) :

```rust
pub struct FrameChoice {
    pub frame_id: String,              // "" = aucun cadre
    pub line1: String, pub line2: String,
    pub event_name: String, pub event_folder: String, pub event_date: NaiveDate,
}
/// Ordre : événement en cours → défaut général. Ne touche pas au disque.
pub fn frame_choice(settings: &Settings, now: NaiveDateTime) -> FrameChoice;
```

Puis, côté fichiers (`frame_store`) :

```rust
/// Chemin du PNG à imprimer pour ce choix. Le rend et le met en cache si besoin.
/// Appelé depuis un fil de travail uniquement.
pub fn resolve(roots: &FrameRoots, choice: &FrameChoice, theme: &LoadedTheme) -> io::Result<Option<PathBuf>>;
```

- Cadre *image* : chemin de son `frame.png` (pas de cache).
- Cadre *modèle* : fichier de cache nommé par l'empreinte de tout ce qui influence le
  rendu (paramètres du modèle, textes remplacés, date, identifiant et date de
  modification du logo utilisé, `PRINT_SIZE`). Empreinte = 16 caractères hexadécimaux
  d'un hachage simple (FNV-1a 64 bits écrit à la main, pas de dépendance).
  Présent → on le renvoie ; absent → `render_frame`, écriture atomique, puis on le renvoie.
- Cadre introuvable ou invalide → on passe au suivant dans l'ordre D5 et on le note
  dans le journal. Jamais d'erreur bloquante.
- Élagage du cache : garder les 40 fichiers les plus récents.

### 4.5 Branchement dans la borne

| Moment | Action |
| --- | --- |
| Démarrage, retour au live (`enter_ready`), sortie de la maintenance | Calculer `frame_choice`. S'il diffère du choix chargé : demander au fil photo de résoudre et charger le cadre (`PhotoCommand` gagne une variante `LoadFrame(FrameChoice)` ; réponse `PhotoEvent::Frame(Option<Arc<RgbaImage>>, FrameChoice)`). |
| Une fois par minute, borne sur le live ou en veille, **aucune séance en cours** | Même vérification (changement de créneau du planning). Utiliser le réveil déjà programmé pour la veille : pas de minuterie supplémentaire tant que le prochain changement de créneau est loin — programmer `request_repaint_after` sur l'heure exacte du prochain début ou de la prochaine fin de créneau. |
| Début de séance | Le cadre chargé est figé pour la séance (D6). |
| Cadre en cours de chargement au moment d'une photo | Utiliser le cadre précédent (toujours présent en mémoire). |

`Kiosk::frame_rgba` (déjà existant) reste l'unique cadre en mémoire.
`Kiosk::reload_frame` et `Settings.frame_path` sont remplacés par ce mécanisme.
Le guide de cadrage du live (`print_area`) continue de fonctionner tel quel (il ne
dépend que des dimensions du cadre).

## 5. Nouveaux modules

Dans la bibliothèque (`src/`), sans egui, donc testables seuls.

### 5.1 `src/frame_config.rs`

`FrameConfig` (`schema_version`, `name`, `kind`, `template: Option<Template>`), énumérations
`FrameKind { Image, Template }`, `Style { None, Border, Band, Polaroid }`,
`LogoPlacement { None, Left, Center, Right }`, `LogoSource { Theme, Frame }`.
`Deserialize` + `Serialize`, `deny_unknown_fields`.
`load(root, id) -> io::Result<LoadedFrame>` avec les mêmes garde-fous que
`theme_config::load` (pas de lien symbolique, dossier contenu dans la racine, PNG borné,
couleurs `#RRGGBB`, bornes numériques, textes sans caractère de contrôle).
`discover_all(installed_root) -> (Vec<LoadedFrame>, rejetés)`, 128 cadres au plus.

### 5.2 `src/frame_render.rs`

```rust
pub struct FrameText<'a> { pub line1: &'a str, pub line2: &'a str }

/// Cadre RGBA de taille PRINT_SIZE, transparent dans la fenêtre photo.
pub fn render_frame(template: &Template, logo: Option<&RgbaImage>, text: &FrameText) -> RgbaImage;

/// Remplace {event} {date} {ligne1} {ligne2}.
pub fn expand(pattern: &str, choice: &FrameChoice) -> String;

pub struct FrameReport {
    pub size: (u32, u32),
    pub window_ratio: f32,                      // part de pixels laissant voir la photo (alpha < 32)
    pub window: Option<(u32, u32, u32, u32)>,   // rectangle englobant de la zone transparente
    pub ratio_mismatch: bool,                   // proportions ≠ 3:2 à plus de 1 %
}
pub fn inspect_frame(frame: &RgbaImage) -> FrameReport;

/// Import d'un cadre : PNG avec canal alpha, borné avant décodage, ramené à PRINT_SIZE.
pub fn prepare_frame_import(path: &Path) -> io::Result<RgbaImage>;
```

Géométrie des modèles sur un tirage `W×H` = 1800×1200 :

| Style | Fenêtre photo (transparente) | Zones opaques |
| --- | --- | --- |
| `none` | Tout | Rien (PNG entièrement transparent) |
| `border` | `(border, border)` → `(W−border, H−border)`, coins arrondis de `radius` | Bordure de couleur `color`. Si `logo ≠ none` : logo dans un cartouche de la couleur du cadre, dans le coin bas correspondant, hauteur 110 px, marge 24 px. Pas de texte. |
| `band` | `(border, border)` → `(W−border, H−band)`, coins arrondis de `radius` | Bordure + bandeau bas de hauteur `band`. Dans le bandeau : logo (hauteur `band − 60`, largeur 420 px au plus) à gauche, au centre ou à droite ; `line1` en Fraunces 700 (taille `band × 0,34`) et `line2` en DM Sans 500 (taille `band × 0,19`), couleur `text_color`, du côté opposé au logo (centrées sous le logo si `logo = center`, centrées seules si `logo = none`). |
| `polaroid` | `(border, border)` → `(W−border, H−band)`, sans arrondi | Comme `band`, `border` ≥ 48, liseré de 2 px à 12 % de noir autour de la fenêtre. |

Règles de rendu :

- La fenêtre est transparente : la photo, qui remplit tout le tirage, apparaît dedans.
  Le cadre recouvre donc les bords de la photo ; c'est voulu.
- Bord de la fenêtre lissé sur 1 px (pas d'effet d'escalier sur les arrondis).
- Texte : `ab_glyph`, même méthode que `ui::BigNumeral` (boîte, couverture par glyphe,
  mélange alpha). Texte trop long : réduire la taille par pas de 6 %, 5 pas au plus,
  puis couper avec « … ». Jamais de débordement hors du bandeau.
- Logo : `fast_image_resize` (CatmullRom), proportions conservées, mélange alpha.
- Fonction pure : aucun accès disque.
- Polices : les déplacer dans `src/fonts.rs` (`include_bytes!`), partagées avec `kiosk/ui.rs`.
- Objectif : ≤ 80 ms sur le PC cible.

### 5.3 `src/frame_store.rs`

```rust
pub struct FrameRoots { pub installed: PathBuf, pub user: PathBuf, pub cache: PathBuf }
pub enum Origin { Bundled, User }
pub struct FrameEntry { pub frame: LoadedFrame, pub origin: Origin }

pub fn list(roots: &FrameRoots) -> io::Result<(Vec<FrameEntry>, usize)>;
pub fn free_id(roots: &FrameRoots, name: &str) -> io::Result<String>;
pub fn create_template(roots: &FrameRoots, name: &str, template: &Template, logo: Option<&RgbaImage>) -> io::Result<String>;
pub fn create_image(roots: &FrameRoots, name: &str, image: &RgbaImage) -> io::Result<String>;
pub fn update(roots: &FrameRoots, id: &str, config: &FrameConfig, image: Option<&RgbaImage>, logo: Option<Option<&RgbaImage>>) -> io::Result<()>;
pub fn duplicate(roots: &FrameRoots, source_id: &str, new_name: &str) -> io::Result<String>;
pub fn delete(roots: &FrameRoots, id: &str, used_by: &[String]) -> io::Result<()>;   // refuse si utilisé
pub fn resolve(roots: &FrameRoots, choice: &FrameChoice, theme: &LoadedTheme) -> io::Result<Option<PathBuf>>;
pub fn prune_cache(roots: &FrameRoots, keep: usize);
pub fn cleanup(roots: &FrameRoots);     // dossiers provisoires orphelins, au démarrage
```

- Écritures atomiques : dossier provisoire `staging-…` puis échange, ancienne version
  déplacée dans `BoothSoft/backups/frames/<id>/` (3 conservées, module `backups`).
- `delete` : refuse un cadre livré ; refuse un cadre utilisé (`used_by` = noms des
  événements qui le désignent + « cadre par défaut ») avec le message
  « Ce cadre est utilisé par : … ».
- `update` d'un cadre *modèle* invalide les fichiers de cache de ce cadre (ils ne
  seront plus trouvés, l'empreinte ayant changé ; `prune_cache` les supprimera).

### 5.4 Changements dans l'existant

| Fichier | Changement |
| --- | --- |
| `src/compositor.rs` | `pub const PRINT_SIZE: (u32, u32) = (1800, 1200);` — `compose_owned` **inchangé**. |
| `src/model.rs` | Champs du § 4.3 ; `frame_choice` ; `frame_path` ignoré. |
| `src/photo_worker.rs` | `PhotoCommand::LoadFrame`, `PhotoEvent::Frame`. Aujourd'hui `PhotoCommand` est une structure : la transformer en énumération `{ Develop(DevelopJob), LoadFrame(FrameChoice) }`. |
| `src/kiosk.rs` | `frame_choice_loaded: Option<FrameChoice>` ; vérification aux moments du § 4.5 ; suppression de `reload_frame` et de l'écriture de `frame_path`. |
| `src/kiosk/admin.rs` | `apply_selected_theme` n'écrit plus `frame_path` ; nouvelle page Cadres ; carte Cadre dans chaque événement. |
| `src/theme_config.rs` | `assets.frame` devient facultatif (`Option<String>`) : cadre de secours. Les anciens thèmes restent valides. |
| `src/lib.rs`, `src/main.rs` | Déclarer les modules ; `frame_store::cleanup` au démarrage ; dossier `frames/` cherché à côté de l'exécutable puis dans le dossier courant (comme `installed_themes_root`). |
| `themes/` | Supprimer `mariage/` et `anniversaire/` ; renommer l'affichage de `default` en « Party ». |
| `frames/` (nouveau, livré) | `sans-cadre`, `bandeau`, `polaroid`, `mariage`, `anniversaire` (ces deux derniers : PNG repris des anciens thèmes). |
| `scripts/theme-art/generate.py` | Produire les cadres dans `frames/` au lieu des thèmes. |

## 6. Interface

Composants et règles : `MAINTENANCE_PATTERNS.md` (confirmation avant de quitter,
historique, suppression confirmée, tout au doigt). Tailles pour 1280×800.

### 6.1 Dans un événement (Maintenance → Événements)

Chaque événement du planning gagne une carte **Cadre** sous ses horaires ; l'événement
par défaut aussi.

```
┌ Mariage Léa & Hugo ─────────────────────────────────── À venir ──────────┐
│ Nom         [ Mariage Léa & Hugo            ]            [ Supprimer ]   │
│ Date        −  sam. 14 nov. 2026  +                                      │
│ Horaires    −  18:00  +   →   −  03:00  +      Se termine le lendemain.  │
│ ─────────────────────────────────────────────────────────────────────── │
│ Cadre du tirage                                                          │
│ ┌──────────────────────┐   Bandeau                           [ Changer ] │
│ │  aperçu du tirage    │   Ligne 1  [ Léa & Hugo                     ]   │
│ │  300×200, photo      │   Ligne 2  [ 14 novembre 2026               ]   │
│ │  d'exemple + cadre   │   (vides : nom et date de l'événement)          │
│ └──────────────────────┘   [ Voir en grand ]                             │
└──────────────────────────────────────────────────────────────────────────┘
```

- **Aperçu du tirage** : photo d'exemple (dernière photo de l'événement, sinon
  `camera::demo_frame()`) + cadre rendu avec les textes de l'événement. Recalculé par le
  fil de travail 300 ms après la dernière frappe ; pendant le calcul, l'ancien aperçu
  reste affiché.
- **Changer** : ouvre le **sélecteur de cadre** (§ 6.2).
- **Lignes 1 et 2** : visibles seulement si le cadre choisi est un modèle qui utilise
  `{ligne1}` / `{ligne2}`. Texte grisé = valeur automatique (nom, date).
- **Voir en grand** : tirage plein écran sur fond sombre, toucher pour fermer.
- Ces réglages font partie de `Settings` : ils suivent la barre « Modifications non
  enregistrées » et la confirmation avant de quitter.

### 6.2 Sélecteur de cadre

Fenêtre plein écran (entrée par `ui::appear`), grille de vignettes 240×160 :

```
┌ Choisir le cadre de « Mariage Léa & Hugo » ──────────────── [ Fermer ] ──┐
│ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐        │
│ │Sans    │ │Bandeau │ │Polaroïd│ │Mariage │ │Anniv.  │ │Léa &   │        │
│ │cadre   │ │        │ │        │ │        │ │        │ │Hugo ✓  │        │
│ └────────┘ └────────┘ └────────┘ └────────┘ └────────┘ └────────┘        │
│ ┌────────┐ ┌────────┐                                                    │
│ │   +    │ │   ⬆    │                                                    │
│ │Créer   │ │Importer│                                                    │
│ └────────┘ └────────┘                                                    │
│                                    [ Modifier ce cadre ] [ Choisir ]     │
└──────────────────────────────────────────────────────────────────────────┘
```

- Chaque vignette montre le cadre **avec les textes de l'événement** sur la photo
  d'exemple (rendu en 480×320 par le fil de travail, 12 à la fois, mis en mémoire le
  temps de la fenêtre). Damier gris clair sous la photo pour le cadre « Sans cadre ».
- Badge « Livré » / « Perso ». Le cadre actuel porte une coche.
- **Choisir** : affecte le cadre à l'événement (modification de `Settings`, à enregistrer).
- **Créer** : demande un nom, ouvre l'éditeur de cadre (§ 6.4) sur un modèle `band`.
- **Importer** : choisir un PNG (boîte du système, puis explorateur tactile quand il
  existera — voir `THEME_EDITOR.md` § 8), rapport d'import (§ 6.5), nom, création.
- **Modifier ce cadre** : ouvre l'éditeur ; pour un cadre livré, copie d'abord
  (« Ce cadre est fourni avec BoothSoft. Une copie modifiable va être créée. »).

### 6.3 Page Cadres (nouvelle entrée du menu de la maintenance)

Même grille que le sélecteur, hors du contexte d'un événement (textes d'exemple :
« Prénom & Prénom », date du jour), avec les actions : Créer, Importer, Modifier,
Dupliquer, Supprimer, et sous chaque cadre la mention « Utilisé par : Mariage Léa &
Hugo, cadre par défaut » ou « Non utilisé ».

Carte en tête de page : **Cadre par défaut** (hors planning et événements sans cadre),
avec le même bloc que le § 6.1 (aperçu, Changer, lignes 1 et 2).

### 6.4 Éditeur de cadre

```
┌─ Cadre : Léa & Hugo ───────────────────────────────────── [ ← Cadres ] ──┐
├──────────────────────────────────────────┬───────────────────────────────┤
│ Nom           [ Léa & Hugo            ]  │        APERÇU DU TIRAGE       │
│                                          │  ┌─────────────────────────┐  │
│ Style   ( Sans | Bordure | Bandeau |     │  │                         │  │
│           Polaroïd )                     │  │   photo d'exemple       │  │
│ Couleur du cadre   ● #FFFFFF             │  │   + cadre               │  │
│ Couleur du texte   ● #263A2C             │  │                         │  │
│ Bordure   − 36 px +                      │  ├─────────────────────────┤  │
│ Bandeau   − 190 px +                     │  │ logo   Léa & Hugo       │  │
│ Arrondi   − 0 px +                       │  │        14 novembre 2026 │  │
│ Logo      ( Aucun | Gauche | Centre |    │  └─────────────────────────┘  │
│             Droite )                     │  Textes d'essai               │
│ Logo utilisé ( Celui du thème |          │  Ligne 1 [ Léa & Hugo      ]  │
│                Un autre… ) [ Choisir… ]  │  Ligne 2 [ 14 novembre 2026]  │
│ Ligne 1   [ {ligne1}               ]     │                               │
│           (+ Nom) (+ Date) (+ Ligne 1)   │  [ Voir en grand ]            │
│ Ligne 2   [ {ligne2}               ]     │                               │
│           (+ Nom) (+ Date) (+ Ligne 2)   │                               │
│ ┌ Historique ─────────────────────────┐  │                               │
│ │ (3 dernières versions, Restaurer)   │  │                               │
│ └─────────────────────────────────────┘  │                               │
├──────────────────────────────────────────┴───────────────────────────────┤
│ Modifications non enregistrées                    [ Annuler ] [ Enregistrer ] │
└───────────────────────────────────────────────────────────────────────────┘
```

- Pour un cadre **image** : seuls le nom, l'aperçu, « Remplacer l'image… », le rapport
  d'import et l'historique sont affichés.
- Les réglages numériques sont des `stepper` (pas de 4 px, appui long = répétition) ;
  les couleurs ouvrent le sélecteur de couleur de `THEME_EDITOR.md` § 7.4 (composant
  partagé, à ranger dans `src/kiosk/admin/color_picker.rs`).
- Boutons « + Nom », « + Date », « + Ligne 1 », « + Ligne 2 » : insèrent la variable à
  la fin du champ (utile sans clavier).
- « Textes d'essai » : ne sont pas enregistrés ; ils ne servent qu'à l'aperçu. Quand
  l'éditeur est ouvert depuis un événement, ils sont préremplis avec ses textes.
- Aperçu : image 720×480, recalculée au relâchement d'un réglage ou 150 ms après la
  dernière frappe. Aperçu immédiat simplifié pendant le calcul : rectangles de la
  bordure et du bandeau dessinés au `Painter` par-dessus l'ancienne image.
- Enregistrer : `frame_store::update` dans le fil de travail ; si le cadre est celui de
  l'événement en cours, il est rechargé tout de suite pour les prochains tirages.
- Quitter avec des modifications : fenêtre de `MAINTENANCE_PATTERNS.md` § 2
  (« Quitter sans enregistrer ? »).

### 6.5 Rapport d'import d'un cadre PNG

| Constat (`inspect_frame`) | Réaction |
| --- | --- |
| Pas de canal alpha, ou image entièrement opaque | Refus : « Un cadre doit être transparent là où la photo apparaît. » |
| `window_ratio < 0,30` | Avertissement : « Ce cadre cache plus des deux tiers de la photo. » |
| `window_ratio > 0,98` | Avertissement : « Ce cadre est presque invisible. » |
| `ratio_mismatch` | Information : « Le cadre a été recadré au format 10×15. » |
| Fichier > 16 Mo ou > 4096 px | Refus : « Cette image est trop lourde (16 Mo et 4096 px au plus). » |
| Pas un PNG | Refus : « Un cadre doit être un fichier PNG transparent. » |

## 7. Fil de travail et budgets

- Les opérations de la maintenance sur les cadres passent par le fil `bs-theme` décrit
  dans `THEME_EDITOR.md` § 6.2 (le renommer `bs-studio` puisqu'il sert aux deux), une
  opération à la fois, file bornée (4), réveil par `request_repaint_after(1 ms)`.
- Le chargement du cadre effectif pour les tirages passe par le fil photo (§ 4.5).

| Mesure | Budget |
| --- | --- |
| CPU au repos, pages Cadres / éditeur ouverts | 0 % |
| RAM supplémentaire, sélecteur ouvert (12 vignettes 480×320) | ≤ 12 Mo, rendus à la fermeture |
| RAM supplémentaire, éditeur de cadre ouvert | ≤ 15 Mo |
| RAM en fonctionnement invité | Inchangée : un seul cadre en mémoire (`frame_rgba`, ≈ 8,6 Mo) |
| Rendu d'un cadre modèle | ≤ 80 ms (fil de travail) |
| Changement de créneau → nouveau cadre prêt | ≤ 2 s, sans gel de l'interface |

## 7 bis. Tirage en portrait : un cadre par orientation

Le tirage 10×15 peut être imprimé **en paysage (15×10, 1800×1200)** ou **en portrait
(10×15, 1200×1800)**. Les deux orientations demandent un cadre différent : un cadre
paysage ne se pose pas sur une photo portrait. Chaque événement a donc **deux
emplacements de cadre** : un pour le paysage, un pour le portrait.

> **Réalisé en partie (15/11/2026).** Orientation Paysage / Portrait / Au choix des
> invités (`Settings::print_orientation`), cadre portrait du thème (`assets.frame_portrait`,
> livré pour Party) ou PNG importé en maintenance (Impression › Format du tirage, 3 anciens
> gardés dans `frames/history/`), fenêtre détectée (P4), guide de cadrage du live sur la
> fenêtre, impression tournée d'un quart de tour sans déformation (P5, Windows : GDI ;
> macOS / Linux : `lp -o fit-to-page`). Reste : cadres par événement (F1), caméra montée
> à la verticale (P6).

### 7 bis.1 Décisions

| # | Décision | Raison |
| --- | --- | --- |
| P1 | L'orientation se règle **par événement** : *Paysage* (défaut), *Portrait*, ou *Au choix de l'invité*. L'événement par défaut a le même réglage. | Un mariage peut vouloir du portrait, une soirée d'entreprise du paysage. |
| P2 | Un cadre a une orientation, déduite de ses dimensions (`largeur > hauteur` = paysage). La bibliothèque filtre les cadres selon l'emplacement choisi. Les cadres *modèles* (bordure, bandeau, polaroïd) sont rendus dans les deux orientations à partir des mêmes paramètres. | Un modèle sert partout ; un PNG importé n'a qu'une orientation. |
| P3 | `compositor::PRINT_SIZE` devient `print_size(orientation)` : (1800, 1200) ou (1200, 1800). | Une seule source de vérité. |
| P4 | La photo est **placée dans la zone transparente du cadre** (sa « fenêtre »), détectée automatiquement au chargement (`compositor::detect_window` : boîte englobante de la zone transparente qui contient le centre du tirage). Un cadre transparent jusqu'aux bords reste « pleine page ». Le reste du tirage est du papier blanc sous le cadre. *(Révisé le 15/11/2026 : auparavant la photo remplissait tout le tirage.)* | Les cadres dessinés avec une fenêtre (ex. portrait 1200×1800, fenêtre 1008×1200 à x 96, y 300) gardent toute la photo visible. |
| P5 | Le fichier imprimé est **tourné de 90° au moment de l'impression** quand son orientation diffère de celle du papier (la DNP QW410 imprime sur un papier 15×10 en paysage). Le fichier enregistré (`photo-NN-cadre.jpg`) reste dans le bon sens. | Aujourd'hui `printer.rs` étire l'image sur toute la page (`StretchDIBits` sur `HORZRES × VERTRES`) : une image portrait y serait **déformée**. |
| P6 | Réglage de maintenance **« Caméra montée à la verticale »** (aucune rotation / 90° / 270°). Quand la caméra est tournée, le live et la photo sont tournés (dans le shader pour le live, dans le fil photo pour la capture). | Recadrer une image paysage en portrait ne garde que 44 % de sa largeur : en webcam 1280×720, la photo portrait ne ferait que 480×720 px, agrandie ×2,5 pour le tirage — floue. Tourner la caméra garde toute la définition. Avec le Canon 2000D (6000×4000), le recadrage reste acceptable. |

### 7 bis.2 Modèle

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation { #[default] Landscape, Portrait }

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrientationChoice { #[default] Landscape, Portrait, Guest }

// EventSlot (en plus du § 4.3)
pub orientation: OrientationChoice,
pub frame_portrait: String,          // vide = cadre portrait par défaut
// Settings (en plus du § 4.3)
pub default_orientation: OrientationChoice,
pub default_frame_portrait: String,  // défaut : "bandeau" (rendu en portrait)
pub camera_rotation: u16,            // 0 | 90 | 270
```

`FrameChoice` (§ 4.4) gagne `orientation: Orientation` ; `frame_choice(settings, now,
guest_pick: Option<Orientation>)` choisit l'emplacement selon le réglage de
l'événement et, pour *Au choix de l'invité*, selon le choix fait sur le live.
Le cache de rendu inclut l'orientation dans l'empreinte.

### 7 bis.3 Borne

- **Live** : le guide de cadrage (`kiosk.rs::print_area`) dépend déjà des dimensions du
  cadre chargé : avec un cadre portrait, les bandes hors tirage apparaissent à gauche
  et à droite automatiquement. À vérifier en capture d'écran, rien d'autre à coder.
- **Au choix de l'invité** : petit sélecteur sur le live, à droite du sélecteur de mode,
  avec deux pictogrammes (rectangle couché / rectangle debout) ; paysage présélectionné,
  remis à zéro à chaque séance. Le cadre correspondant est chargé par le fil photo
  (les deux cadres de l'événement sont gardés en mémoire quand ce choix est actif :
  +8,6 Mo, budget accepté).
- **Aperçu, choix de rafale, remerciement** : `ui::fit` gère déjà les images portrait ;
  vérifier la mise en page en capture (barre d'actions, vignettes).
- **Impression** (`printer.rs`) :
  - Windows : lire l'orientation de la page (`HORZRES` > `VERTRES` ⇒ paysage) ; si
    elle diffère de celle de l'image, tourner l'image de 90° dans le fil d'impression
    avant `StretchDIBits`. Garder les proportions : jamais d'étirement si le rapport
    image / page diffère de plus de 2 % (centrer avec marges blanches et journaliser).
  - macOS / Linux : même rotation avant d'envoyer le fichier à `lp` (fichier
    temporaire dans `print-exports/`), plus `-o fit-to-page`.
- **Exemplaire unique en mémoire** : la rotation se fait sur une copie temporaire,
  libérée après l'envoi.

### 7 bis.4 Maintenance

- Dans chaque événement, carte **Cadre du tirage** : un `segmented` « Paysage · Portrait ·
  Au choix des invités », puis un ou deux blocs d'aperçu (§ 6.1) selon le choix, chacun
  avec son bouton « Changer » qui ouvre le sélecteur filtré sur la bonne orientation.
- Page Cadres : filtre « Tous · Paysage · Portrait » ; pour un cadre modèle, aperçu dans
  les deux orientations côte à côte.
- Page Caméra & écran : réglage « Caméra montée » (Normale · Tournée à droite · Tournée
  à gauche) avec aperçu immédiat du live.
- Si l'orientation portrait est choisie avec une caméra non tournée et une définition
  de photo inférieure à 1200 px de large après recadrage : avertissement
  « En portrait, la photo sera agrandie ×N : tournez la caméra pour plus de netteté. »

### 7 bis.5 Tests

- `print_size` ; `frame_choice` pour les trois réglages ; rendu d'un modèle dans les deux
  orientations (fenêtre transparente au bon endroit) ; rotation à l'impression
  (image portrait sur page paysage → tournée, rien d'étiré) ; composition d'une photo
  paysage dans un cadre portrait (photo recadrée au centre, tirage rempli).
- Scénario Xvfb : événement en portrait, photo en mode démo, `photo-01-cadre.jpg` en
  1200×1800 avec le cadre portrait ; capture du live (bandes latérales) et de l'aperçu.

## 8. Lots

| Lot | Contenu | Effort | Dépend de |
| --- | --- | --- | --- |
| F1.1 — Socle | `PRINT_SIZE`, `fonts.rs`, `frame_config`, `frame_render` (les 4 styles, variables, `inspect_frame`, import), `frame_store` (liste, création, mise à jour, duplication, suppression, `resolve`, cache), cadres livrés dans `frames/`. Tests. Rien de visible. | L | — |
| F1.2 — Cadre par événement | Champs de `EventSlot` et `Settings`, `frame_choice`, branchement du § 4.5 (fil photo, vérification au changement de créneau), migration (§ 9), retrait de `frame_path` et des thèmes `mariage` / `anniversaire`. | M | F1.1 |
| F1.3 — Choisir | Carte Cadre dans chaque événement et dans l'événement par défaut, sélecteur de cadre, aperçu du tirage, « Voir en grand ». | M | F1.2, M1.1 |
| F1.4 — Créer et modifier | Page Cadres, éditeur de cadre, import PNG avec rapport, dupliquer, supprimer, historique (3 versions). | L | F1.3, M1.2 |
| F1.5 — Portrait | § 7 bis : orientation par événement, deux emplacements de cadre, modèles rendus dans les deux sens, rotation à l'impression, caméra tournée, choix par l'invité. | M | F1.3 |

## 9. Migration des installations existantes

À faire une fois, au premier démarrage de la nouvelle version (`Settings::migrate`,
`defaults_version` 3) :

1. Si `theme_id` vaut `mariage` ou `anniversaire` : `default_frame` = le cadre livré du
   même nom, puis `theme_id` = `default` (« Party »).
2. Sinon, si le thème actif est un thème utilisateur avec un `assets/frame.png`
   différent de celui d'un thème livré : créer un cadre *image* utilisateur
   « Cadre de <nom du thème> » à partir de ce fichier et en faire `default_frame`.
3. Sinon `default_frame = "bandeau"`.
4. Tous les événements du planning : `frame` vide (ils prennent le cadre par défaut).
5. `frame_path` n'est plus lu.

Les tirages déjà enregistrés (`photo-NN-cadre.jpg`) ne sont pas touchés.

## 10. Tests

| Module | Cas |
| --- | --- |
| `model` | `frame_choice` : événement en cours avec cadre ; événement sans cadre → défaut ; hors planning → défaut ; lignes vides → nom et date ; créneau à cheval sur minuit. Migration : les trois branches du § 9. |
| `frame_config` | Lecture / écriture identiques ; champs inconnus refusés ; bornes ; `kind = image` sans `frame.png` refusé ; lien symbolique refusé. |
| `frame_render` | Taille = `PRINT_SIZE` ; centre transparent, coins opaques, bandeau opaque ; `inspect_frame` retrouve la fenêtre à ± 2 px pour chaque style ; `none` → entièrement transparent ; texte très long réduit puis coupé sans panique ; `expand` avec chaque variable et avec une accolade isolée ; import : PNG sans alpha refusé, image 8000×8000 refusée avant décodage, fichier tronqué → erreur sans panique. |
| `frame_store` | `resolve` : cache créé au premier appel, réutilisé au second (même fichier, pas de nouveau rendu), nouveau fichier si un texte change ; cadre absent → repli sur le suivant ; `delete` refuse livré et utilisé ; `update` garde 3 sauvegardes ; `prune_cache`. |
| `compositor` | Non régression : la photo couvre toujours tout le tirage sous le cadre (test existant à conserver, en ajouter un avec un cadre `band`). |

Scénarios Xvfb (captures à regarder) :

1. Deux événements le même jour avec deux cadres : photo en mode démo dans le premier
   créneau puis dans le second (horloge avancée par un réglage de test
   `BOOTHSOFT_NOW=2026-11-14T19:00`, à ajouter) : les deux `photo-01-cadre.jpg` portent
   chacun leur cadre et leurs textes.
2. Événement sans cadre : le tirage porte le cadre par défaut avec le nom et la date de
   l'événement.
3. Créer un cadre Bandeau, l'affecter, enregistrer, photo : cadre présent sur le tirage.
4. Modifier un cadre puis « ← Cadres » : la fenêtre « Quitter sans enregistrer ? »
   apparaît ; tester ses trois boutons.
5. Restaurer une version de l'historique : l'aperçu revient à l'ancien réglage.

## 11. Critères d'acceptation

- [ ] Chaque événement du planning a son cadre, choisi dans sa propre carte ; la borne
      l'applique seule à l'heure de l'événement, sans passage en maintenance.
- [ ] Un même modèle de cadre donne, sans rien refaire, un tirage au nom et à la date de
      chaque événement ; deux lignes de texte libres par événement.
- [ ] La photo remplit tout le tirage sous le cadre (aucune marge blanche ajoutée).
- [ ] On peut créer, importer, modifier, dupliquer, supprimer un cadre depuis la borne.
- [ ] Un événement en portrait imprime un 10×15 debout avec son cadre portrait, sans déformation ;
      chaque événement a un cadre paysage et un cadre portrait.
- [ ] Le thème ne contient plus de choix de cadre ; changer de thème ne change pas le
      cadre des événements.
- [ ] Les thèmes « Mariage » et « Anniversaire » n'existent plus ; leurs cadres sont
      dans la bibliothèque.
- [ ] Quitter un éditeur modifié demande confirmation ; 3 versions d'historique
      restaurables par cadre.
- [ ] Aucun rendu de cadre pendant une prise de vue ; RAM invité inchangée ; 0 % de CPU
      au repos ; tests et clippy passent ; `docs/ARCHITECTURE.md` mis à jour.

## 12. Questions ouvertes (choix par défaut si personne ne répond)

| Question | Choix par défaut |
| --- | --- |
| Un événement peut-il aussi avoir son propre thème ? | Non : le thème est celui de l'entreprise. |
| Le logo du client sur le cadre ? | Oui, par cadre (`logo_source = "frame"`, fichier `logo.png` du cadre). |
| Que faire du cadre à la fin de l'événement ? | Rien : il reste dans la bibliothèque, réutilisable ; le cache est élagué seul. |
| Texte vertical, photo de fond dans le bandeau, plusieurs logos ? | Hors périmètre : importer un PNG fait par un graphiste. |
