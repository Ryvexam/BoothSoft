# Spécification — Éditeur de thèmes intégré (chantier T1)

Statut : **à développer**. Lis d'abord `AGENTS.md`, `docs/ARCHITECTURE.md` § 7 et § 8,
puis `docs/specs/MAINTENANCE_PATTERNS.md` (vocabulaire, confirmation avant de quitter,
historique à 3 versions) : ce document s'appuie dessus et ne les répète pas.

## 1. Ce qu'est un thème — et ce qu'il n'est pas

Un **thème** est l'habillage de la borne aux couleurs de **l'entreprise** qui
l'exploite : couleurs, logo, écran d'accueil, textes des boutons. Un loueur de bornes fait
tous types d'événements avec **le même thème**, celui de sa marque.

| Le thème contient | Le thème ne contient pas |
| --- | --- |
| 6 couleurs | Le cadre des tirages → il appartient à **l'événement** (`EVENT_FRAMES.md`) |
| Le logo de l'entreprise | Le nom ou la date d'un événement |
| Le fond de l'écran d'accueil (veille) | Un style « mariage » ou « anniversaire » |
| Les textes des écrans invités | Les réglages de la borne (impression, rafale…) |
| Un cadre de **secours** (`assets/frame.png`, facultatif), jamais choisi par l'utilisateur | |

**Un seul thème est livré** avec BoothSoft (projet open source) : **Party**
(`default`), générique et festif, celui d'une installation neuve. Chaque exploitant
crée le thème de sa marque avec cet éditeur ou l'importe en ZIP ; il reste chez lui.
Le thème d'une marque est dans ce cas : privé, hors du dépôt, installé dans
`BoothSoft/themes/<id>/` (fiche O3 de la feuille de route).

## 2. Objectif

Depuis la maintenance, sans autre outil :

1. **Créer** un thème (à partir de zéro ou d'un thème existant).
2. **Modifier** un thème : nom, couleurs, logo, fond d'accueil, textes.
3. **Modifier un thème livré** : par une copie personnelle (les fichiers livrés sont en
   lecture seule et seraient écrasés à la mise à jour).
4. **Voir le résultat** avant d'enregistrer, sur les vrais écrans invités.
5. **Dupliquer, exporter (ZIP), supprimer**, et **revenir à l'une des 3 versions
   précédentes**.

Utilisateur : le propriétaire, à l'atelier (clavier) ou sur la borne (tactile seul ;
la saisie de texte dépend de `TOUCH_KEYBOARD.md`).

### Hors périmètre

- Tout ce qui concerne le cadre des tirages (→ `EVENT_FRAMES.md`).
- Polices personnalisées (Fraunces et DM Sans embarquées suffisent).
- Disposition des écrans (positions, tailles) : seules les couleurs, images et textes changent.
- Banque d'images en ligne, génération d'images.

## 3. Ce qui existe déjà

| Élément | Où | À retenir |
| --- | --- | --- |
| Format, validation | `src/theme_config.rs` | `Config`, `load`, `discover_all`, `load_any`, `user_themes_root`, `valid_theme_id`, `valid_color`. Schéma 1, `deny_unknown_fields`. |
| Import ZIP | `src/theme_import.rs` | Extraction dans un dossier provisoire puis renommage ; refuse un identifiant déjà pris. |
| Thème actif | `src/theme.rs` | `active()`, `replace_active()`, `install(ctx)`. |
| Page Thème | `src/kiosk/admin.rs::page_theme` | Grille de cartes, Appliquer / Importer un ZIP / Actualiser. |
| Application | `admin.rs::apply_selected_theme` | Revalide, enregistre `theme_id`, remplace le thème actif. (Il écrit aussi `frame_path` : supprimé par `EVENT_FRAMES.md`.) |
| Polices | `src/kiosk/ui.rs` | Fraunces 700, DM Sans 500 / 700 (`assets/fonts/`). |
| Illustrations livrées | `scripts/theme-art/generate.py` | Outil de dev (Python). **Ne pas l'appeler depuis l'application.** |
| Composants | `admin.rs` | `card`, `row`, `toggle`, `stepper`, `segmented`, `btn`, `Pal`. |

Contraintes du format (à conserver) : identifiant = nom de dossier, minuscules ASCII,
chiffres, `-`, `_`, 48 caractères au plus ; PNG uniquement, noms en minuscules ASCII,
16 Mo et 4096 px au plus ; aucun lien symbolique ; libellés 40 caractères, titres 140
(`\n` autorisé), nom du thème 80.

## 4. Décisions de conception

| # | Décision | Raison |
| --- | --- | --- |
| D1 | Tout thème créé ou modifié vit dans `BoothSoft/themes/<id>/`. | Le dossier des thèmes livrés est en lecture seule. |
| D2 | « Modifier » un thème livré crée d'abord une copie (`<id>-perso`, « X (personnalisé) »), puis ouvre l'éditeur dessus. | Mises à jour sans perte, retour arrière possible. |
| D3 | L'éditeur travaille sur un **brouillon** (mémoire + dossier provisoire). Rien n'est visible des invités avant l'enregistrement. | Pas de thème à moitié modifié en pleine soirée. |
| D4 | Enregistrement atomique : dossier provisoire, validation par `theme_config::load`, échange de dossiers ; l'ancienne version part dans l'historique (3 conservées). | Coupure de courant sans dégât ; retour arrière. |
| D5 | Schéma 1 si le thème n'utilise aucune nouveauté, schéma 2 sinon. | Compatibilité des ZIP entre bornes. |
| D6 | Le thème ne gère **aucun** cadre. `assets.frame` devient facultatif (cadre de secours) et n'apparaît pas dans l'éditeur. | Thème ≠ cadre. |
| D7 | Toute opération d'image dans un fil de travail. | Règle « rien de lent sur le fil d'interface ». |
| D8 | Aperçu fidèle = les **vraies** fonctions d'écran avec le brouillon appliqué temporairement ; aperçu simplifié dessiné à la main à côté de chaque réglage. | Fidélité sans dupliquer le code des écrans. |
| D9 | Aucune nouvelle dépendance. | Taille, RAM. |
| D10 | Quitter avec des modifications : fenêtre « Quitter sans enregistrer ? » de `MAINTENANCE_PATTERNS.md` § 2. | Rien n'est perdu ni enregistré en silence. |

## 5. Format — évolutions

### 5.1 Arborescence

```
<id>/
├── config.toml
└── assets/
    ├── logo.png
    ├── background.png      (facultatif)
    └── frame.png           (facultatif : cadre de secours, non modifiable dans l'éditeur)
```

### 5.2 Manifeste, schéma 2

Schéma 2 = schéma 1 + `assets.frame` facultatif + table facultative `[design]`.
`theme_config::load` accepte `schema_version` 1 **et** 2.

```toml
schema_version = 2
name = "Ma marque"

[colors]            # inchangé : background, foreground, primary, accent, surface, muted
[assets]
logo = "assets/logo.png"
background = "assets/background.png"     # facultatif
# frame = "assets/frame.png"             # facultatif désormais
[ui]                # inchangé

[design]                          # facultatif, écrit par l'éditeur
background_kind = "gradient"      # "image" | "solid" | "gradient"
background_top = "#FBF8F3"
background_bottom = "#EFE6D8"
```

À faire dans `theme_config.rs` :

- `Assets.frame: Option<String>` (`#[serde(default)]`) ; `LoadedTheme.frame: Option<PathBuf>`.
  Adapter les usages (`kiosk.rs::new`, `admin.rs`) : voir `EVENT_FRAMES.md` § 5.4.
- `Config.design: Option<Design>` avec
  `Design { background_kind: Option<BackgroundKind>, background_top: Option<String>, background_bottom: Option<String> }`,
  `deny_unknown_fields`, couleurs validées par `valid_color`.
- `Serialize` sur `Config`, `Colors`, `Assets`, `Labels`, `Design` ;
  `#[serde(skip_serializing_if = "Option::is_none")]` sur tous les champs facultatifs.
- `pub fn write_manifest(dir: &Path, config: &Config) -> io::Result<()>` (écriture
  atomique par `durable_file::replace`, `toml::to_string_pretty`). Écrit
  `schema_version = 1` si `design` est `None` et `frame` est `Some`, sinon 2.
- `discover` ignore explicitement les dossiers `staging-*` (aujourd'hui il tente de les
  charger et les compte en « rejetés »).

## 6. Nouveaux modules

Dans la bibliothèque (`src/`), sans egui.

### 6.1 `src/fonts.rs`

Les trois `include_bytes!` des polices, partagés entre l'interface et le rendu d'images.

### 6.2 `src/theme_design.rs`

```rust
/// Fond d'écran de veille uni ou en dégradé vertical.
pub fn render_background(top: [u8; 3], bottom: [u8; 3], width: u32, height: u32) -> RgbImage;

pub enum ImportKind { Logo, Background }
/// Décodage PNG ou JPEG borné avant allocation, redimensionnement, sortie RGBA.
pub fn prepare_import(path: &Path, kind: ImportKind) -> io::Result<RgbaImage>;

/// Rapport de contraste WCAG entre deux couleurs (1 à 21).
pub fn contrast(a: [u8; 3], b: [u8; 3]) -> f32;
```

| Type importé | Formats | Traitement |
| --- | --- | --- |
| Logo | PNG, JPEG | 16 Mo au plus ; ramené à 1400×600 au plus, jamais agrandi ; JPEG accepté avec l'avertissement « logo sans transparence ». |
| Fond | PNG, JPEG | Ramené en couverture à 1920×1200 au plus, jamais agrandi ; sortie RGB. |

Limites de décodage identiques à `compositor::load_frame` (dimensions et allocation
bornées **avant** le décodage).

### 6.3 `src/theme_store.rs`

```rust
pub enum Origin { Bundled, User }
pub struct ThemeEntry { pub theme: LoadedTheme, pub origin: Origin }

pub fn list(installed_root: &Path) -> io::Result<(Vec<ThemeEntry>, usize)>;
pub fn free_id(installed_root: &Path, name: &str) -> io::Result<String>;   // accents retirés, suffixe -2, -3…
pub fn duplicate(installed_root: &Path, source_id: &str, new_name: &str) -> io::Result<String>;

pub struct Draft { pub id: String, pub dir: PathBuf, pub config: Config, pub is_new: bool }
/// Dossier provisoire `BoothSoft/themes/staging-edit-<pid>-<n>/`, copie de `source_id`
/// ou squelette (couleurs et logo du thème `default`, pas de fond).
pub fn open_draft(installed_root: &Path, source_id: Option<&str>, name: &str) -> io::Result<Draft>;

impl Draft {
    pub fn put_logo(&mut self, image: &RgbaImage) -> io::Result<()>;
    pub fn put_background(&mut self, image: Option<&RgbImage>) -> io::Result<()>;   // None = retirer
    pub fn validate(&self) -> io::Result<LoadedTheme>;     // écrit config.toml puis theme_config::load
    pub fn commit(self) -> io::Result<LoadedTheme>;        // validation + échange atomique + historique
    pub fn discard(self);
}

pub fn delete(installed_root: &Path, id: &str, active_id: &str) -> io::Result<()>;
pub fn export_zip(installed_root: &Path, id: &str, destination: &Path) -> io::Result<()>;
pub fn cleanup(user_root: &Path);     // orphelins staging-* et *.old-*, au démarrage
```

- Noms de fichiers imposés dans le brouillon : `assets/logo.png`,
  `assets/background.png`. Jamais le nom d'origine.
- `commit` : (1) `validate` — en cas d'échec rien n'est modifié ; (2) si le thème
  existe, `backups::push("themes", id, dossier)` déplace l'ancienne version dans
  l'historique ; (3) renommer le dossier provisoire en `<id>` ; (4) élaguer l'historique
  à 3. Si (3) échoue, remettre l'ancienne version en place.
- `delete` : refuse un thème livré et le thème actif ; la dernière version part dans
  l'historique (récupérable 30 jours).
- `export_zip` : compatible avec `theme_import::import_zip` (racine `<id>/`,
  `config.toml` + `assets/**.png`).
- Prévoir pour chaque fonction une variante interne prenant `user_root` en paramètre
  (comme `theme_import::import_zip_into`) : les tests n'écrivent jamais dans le vrai
  dossier de données.
- Module `src/backups.rs` : défini dans `MAINTENANCE_PATTERNS.md` § 3.

### 6.4 Changements dans l'existant

| Fichier | Changement |
| --- | --- |
| `src/theme_config.rs` | § 5.2. |
| `src/theme_import.rs` | Accepter le schéma 2 et un thème sans `frame`. |
| `src/lib.rs`, `src/main.rs` | Déclarer les modules ; `theme_store::cleanup` au démarrage. |
| `src/kiosk/ui.rs` | `Palette::from_colors(&Colors)` ; polices importées de `boothsoft::fonts`. |
| `src/kiosk.rs` | `theme_editor: Option<Editor>` ; `palette()` et les libellés lisent le brouillon pendant l'aperçu plein écran (§ 8.6). |
| `src/kiosk/admin.rs` | Page Thème revue (§ 8.1). Éditeur dans `src/kiosk/admin/theme_editor.rs`, sélecteur de couleur dans `src/kiosk/admin/color_picker.rs` (partagé avec l'éditeur de cadre). Transformer `admin.rs` en `admin/mod.rs` ; rendre `pub(super)` `card`, `row`, `btn`, `toggle`, `stepper`, `segmented`, `Pal`. |
| `themes/` | Seul `default` est livré (nom affiché « Party »). |

## 7. Architecture de l'éditeur

### 7.1 État

```rust
pub struct Editor {
    id: String, is_new: bool,
    config: Config,                     // brouillon affiché
    saved: Config,                      // dernière version enregistrée (détection des modifications)
    images_changed: bool,               // logo ou fond importé depuis le dernier enregistrement
    tab: Tab,                           // Identite | Couleurs | Accueil | Textes | Historique
    logo: Option<TextureHandle>,        // vignettes ≤ 600 px
    background: Option<TextureHandle>,
    backups: Vec<Backup>,
    busy: Option<&'static str>,
    message: Option<(String, bool)>,    // texte, vrai = erreur
    color_target: Option<ColorField>,
    preview: Option<PreviewScreen>,     // aperçu plein écran ouvert
    worker: StudioWorker,
}
impl Editor { pub fn modified(&self) -> bool { self.config != self.saved || self.images_changed } }
```

`Editor::modified` alimente `Kiosk::unsaved()` (`MAINTENANCE_PATTERNS.md` § 2.2).

### 7.2 Fil de travail `bs-studio`

Créé à l'ouverture d'un éditeur (thème ou cadre), arrêté à sa fermeture. Il **possède**
le `Draft` ; l'interface envoie sa copie de `Config` avec chaque commande. File bornée
(4), une opération à la fois, réveil par `request_repaint_after(1 ms)`.

```rust
enum Job {
    ImportLogo(PathBuf), ImportBackground(PathBuf), RemoveBackground,
    RenderBackground { top: [u8; 3], bottom: [u8; 3] },
    Thumbnails, ListBackups, Restore(String),
    Validate(Config),                // pour l'aperçu plein écran
    Save { config: Config, apply: bool }, Export(PathBuf), Discard,
}
enum Done {
    Assets { logo: Option<RgbaImage>, background: Option<RgbaImage> },
    Backups(Vec<Backup>), Validated(LoadedTheme),
    Saved(LoadedTheme, bool), Exported(PathBuf), Failed(String),
}
```

### 7.3 Budget

| Mesure | Budget |
| --- | --- |
| RAM supplémentaire, éditeur ouvert | ≤ 15 Mo |
| RAM après fermeture | Niveau d'avant (± 2 Mo) |
| CPU au repos, éditeur ouvert | 0 % |
| Toucher → retour visuel | Image suivante |
| Enregistrement | ≤ 1 s, interface non bloquée |

## 8. Interface

Tailles pour 1280×800. Règles communes : `MAINTENANCE_PATTERNS.md` § 5.

### 8.1 Page Thème

```
┌ Thème ─────────────────────────────────────────────────────────────────────┐
│ L'habillage de la borne aux couleurs de l'entreprise.                      │
│ Le cadre des tirages se règle dans chaque événement.                       │
│                                                                            │
│ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐                        │
│ │  logo    │ │  logo    │ │  logo    │ │    +     │                        │
│ │ sur fond │ │          │ │          │ │ Nouveau  │                        │
│ ├──────────┤ ├──────────┤ ├──────────┤ │  thème   │                        │
│ │ Ma marque│ │  Party   │ │  Party   │ │          │                        │
│ │●●● Actif │ │●●● Livré │ │ (perso.) │ │          │                        │
│ └──────────┘ └──────────┘ └──────────┘ └──────────┘                        │
│                                                                            │
│ Thème sélectionné : Party                                                  │
│ [ Appliquer ] [ Modifier ] [ Dupliquer ] [ Exporter ] [ Supprimer ]        │
│ [ Importer un thème (ZIP) ]  [ Actualiser ]                                │
└────────────────────────────────────────────────────────────────────────────┘
```

- Badges : « Actif », « Livré », « Perso ».
- **Modifier** : thème perso → éditeur ; thème livré → « Ce thème est fourni avec
  BoothSoft. Une copie modifiable va être créée. » puis copie et éditeur.
- **Dupliquer** : demande un nom (prérempli « X (copie) »).
- **Exporter** : boîte d'enregistrement du système, nom proposé `<id>.zip`.
- **Supprimer** : désactivé pour un thème livré ou actif, raison affichée ; sinon
  confirmation « Supprimer définitivement “X” ? » avec bouton rouge.
- **Nouveau thème** : demande un nom, ouvre l'éditeur sur un squelette.

### 8.2 Éditeur — disposition

```
┌─[logo]─ Thème : Ma marque ───────────────────────────────── [ ← Thèmes ] ─┐
├──────────────┬──────────────────────────────────┬────────────────────────┤
│ Identité     │  (réglages de l'onglet)          │  APERÇU                │
│ Couleurs   ◀ │                                  │  ┌──────────────────┐  │
│ Accueil      │                                  │  │ écran simplifié  │  │
│ Textes       │                                  │  └──────────────────┘  │
│ Historique   │                                  │  ○ Accueil  ○ Live     │
│              │                                  │  ○ Photo    ○ Merci    │
│              │                                  │  [ Voir en plein écran ]│
├──────────────┴──────────────────────────────────┴────────────────────────┤
│ Modifications non enregistrées   [ Annuler ] [ Enregistrer ] [ Enregistrer et appliquer ] │
└───────────────────────────────────────────────────────────────────────────┘
```

- Onglets 220 px, réglages souples et défilants, aperçu 380 px fixe (sous les réglages
  en dessous de 1100 px de large).
- Barre du bas visible dès que `modified()` est vrai. « Enregistrer et appliquer »
  n'apparaît que si le thème édité n'est pas le thème actif ; si c'est le thème actif,
  « Enregistrer » l'applique d'office.
- Un point d'accent marque les onglets qui contiennent une modification.

### 8.3 Onglet Identité

| Réglage | Composant | Règles |
| --- | --- | --- |
| Nom du thème | Champ de texte | 1 à 80 caractères. L'identifiant (dossier) est fixé à la création et ne change pas. |
| Identifiant | Lecture seule, petit | Pour repérer le dossier. |
| Logo | Vignette 320×140 sur la couleur de fond + [ Remplacer… ] | PNG / JPEG. Avertissements : pas de transparence ; contraste moyen avec le fond < 2. |

### 8.4 Onglet Couleurs

Six lignes : pastille 44 px, nom, code, rôle.

| Champ | Libellé | Rôle affiché |
| --- | --- | --- |
| `background` | Fond | Fond des écrans d'accueil, de photo et de remerciement |
| `foreground` | Texte | Textes courants |
| `primary` | Principale | Grands titres, menu de la maintenance |
| `accent` | Accent | Boutons d'action, sélection |
| `surface` | Cartes | Fenêtres et cartes |
| `muted` | Texte discret | Sous-titres, aides |

**Sélecteur de couleur** (panneau dans la colonne de réglages, composant partagé
`color_picker.rs`) :

- carré saturation / luminosité 280×200 px + bande de teinte 280×36 px, à faire
  glisser au doigt, dessinés par maillage coloré (`egui::Mesh`, pas de texture) ;
- champ `#RRGGBB` (clavier tactile en mode hexadécimal) ;
- 12 suggestions : les 6 couleurs du thème + plus claire / plus foncée de l'accent et
  de la principale ;
- [ Valider ] [ Annuler ].

**Contrôles de lisibilité** (`theme_design::contrast`) :

| Paire | Seuil | Message en dessous |
| --- | --- | --- |
| `foreground` / `background` | 4,5 | « Le texte sera difficile à lire sur le fond. » |
| `primary` / `background` | 3 | « Les titres ressortent peu sur le fond. » |
| `foreground` / `surface` | 4,5 | « Le texte sera difficile à lire dans les fenêtres. » |
| `muted` / `background` | 3 | « Le texte discret est trop pâle. » |

Les avertissements n'empêchent pas d'enregistrer. Bouton « Proposer des couleurs
lisibles » : ajuste la luminosité de `foreground` et `muted` jusqu'aux seuils.
Rangée de 8 **palettes prêtes** (constante du module) qui remplissent les six couleurs.

### 8.5 Onglets Accueil, Textes, Historique

**Accueil** (écran de veille) :

| Réglage | Composant |
| --- | --- |
| Type de fond | `segmented` : Image · Dégradé · Uni |
| Image | Vignette 16:10 + [ Choisir une image… ] [ Retirer ] |
| Dégradé / Uni | Deux pastilles (haut, bas) ouvrant le sélecteur ; « Reprendre les couleurs du thème » |
| Sur-titre, titre, sous-titre, bouton | Mêmes champs que l'onglet Textes, répétés ici |

Pour Dégradé / Uni, `assets/background.png` (1600×1000) est fabriqué par
`render_background` à l'enregistrement et `[design]` est renseigné.

**Textes** :

| Champ | Libellé dans l'éditeur | Longueur | Si vide |
| --- | --- | --- | --- |
| `welcome_eyebrow` | Sur-titre de l'accueil | 40 | « Photobooth » |
| `welcome_title` | Titre de l'accueil (retours à la ligne permis) | 140 | défaut du code |
| `welcome_subtitle` | Sous-titre de l'accueil | 140 | défaut du code |
| `start_label` | Bouton de l'accueil | 40 | « Commencer » |
| `shutter_label` | Sous le déclencheur | 40 | obligatoire → « Prendre la photo » |
| `retake_label` | Bouton Reprendre | 40 | obligatoire → « Reprendre » |
| `finish_label` | Bouton Terminer | 40 | obligatoire → « Terminer » |
| `print_label` | Bouton Imprimer | 40 | « Imprimer » |
| `thanks_title` | Titre du remerciement | 140 | « Merci ! » |
| `thanks_subtitle` | Sous-titre du remerciement | 140 | défaut du code |

Compteur de caractères, saisie bloquée à la limite ; champ facultatif vidé → `None`
(texte par défaut affiché en grisé) ; apostrophe droite remplacée par l'apostrophe
typographique ; bouton « Rétablir les textes par défaut ».

**Historique** : la carte de `MAINTENANCE_PATTERNS.md` § 3.3 (3 versions, Restaurer).

### 8.6 Aperçu

**Simplifié** (colonne de droite, 360×225 px), dessiné avec `Palette::from_colors` :

| Vue | Contenu |
| --- | --- |
| Accueil | Fond, logo, sur-titre, titre, sous-titre, bouton d'accent |
| Live | Rectangle gris-photo, sélecteur « Une photo / Rafale », déclencheur, 6 vignettes |
| Photo | Fond sombre, tirage d'exemple (avec le cadre de l'événement en cours), boutons |
| Merci | Tirage incliné, titre et sous-titre du remerciement |

La vue suit l'onglet (Couleurs → Accueil, Textes → Accueil), modifiable par les quatre
boutons. Mise à jour immédiate.

**Fidèle** (« Voir en plein écran ») :

1. `Job::Validate(config)` ; en cas d'erreur, message et pas d'aperçu.
2. L'interface garde l'`Arc` du thème actif, appelle `theme::replace_active(brouillon)`,
   vide `Kiosk::art` et `Kiosk::admin_logo`.
3. Les vraies fonctions `render_welcome`, `render_review`, `render_thanks` dessinent en
   plein écran, avec un état d'exemple (`review` et `thanks_photo` = photo d'exemple
   composée avec le cadre de l'événement en cours, `thanks_copies = 1`).
4. Bandeau flottant en bas : « Aperçu · Accueil | Photo | Merci · [ Fermer l'aperçu ] ».
   Une zone plein écran absorbe les touchers : aucun bouton invité n'agit. Ajouter un
   drapeau `preview_mode` testé par les actions de ces écrans (aucune écriture disque,
   aucun changement de `self.screen`).
5. À la fermeture — et dans **tous** les chemins de sortie (abandon, retour aux photos,
   erreur, verrouillage automatique) : remettre le thème actif d'origine et vider
   `art`. Centraliser dans `Editor::close_preview`.

### 8.7 Enregistrer

| Action | Effet |
| --- | --- |
| Enregistrer | Fil de travail : fabrique le fond si besoin, écrit `config.toml`, `commit`. L'éditeur reste ouvert sur un nouveau brouillon. Message « Thème enregistré ». |
| Enregistrer et appliquer | Idem, puis `theme_id` enregistré par `save_only`, thème actif remplacé, `theme::install`. Message « Thème enregistré et appliqué ». |
| Thème actif modifié | « Enregistrer » applique d'office. |
| Échec | Message en français, brouillon conservé, thème en place intact. |

| Cause | Message |
| --- | --- |
| Image trop grande / trop lourde | « Cette image est trop lourde (16 Mo au plus). » |
| Format non pris en charge | « Choisissez une image PNG ou JPEG. » |
| Texte trop long ou vide | « Le champ “…” doit contenir entre 1 et N caractères. » |
| Couleur invalide | « Couleur invalide : utilisez la forme #RRGGBB. » |
| Écriture impossible | « Enregistrement impossible : vérifiez l'espace disque. » |

### 8.8 Quitter l'éditeur

Toute sortie avec `modified()` vrai passe par la fenêtre **« Quitter sans enregistrer ? »**
de `MAINTENANCE_PATTERNS.md` § 2, avec ses trois boutons (« Enregistrer et quitter »,
« Quitter sans enregistrer », « Rester sur cette page ») et la liste
« • Thème “<nom>” ». Sorties concernées :

| Sortie | Action d'origine (`PendingLeave`) |
| --- | --- |
| « ← Thèmes » | `CloseEditor` |
| Toucher une autre entrée du menu de la maintenance | `CloseEditor` puis changement de page |
| « Retour aux photos » | `BackToPhotos` |
| « Fermer BoothSoft » | `CloseApp` |
| « Verrouiller maintenant » | `Lock` |
| « Annuler » (barre du bas) | Fenêtre réduite « Abandonner les modifications ? » |
| Verrouillage automatique (10 min) | Pas de fenêtre : brouillon conservé et proposé à la réouverture (§ 4 du même document) |

« Quitter sans enregistrer » appelle `Job::Discard` (suppression du dossier provisoire)
et `close_preview` si besoin. L'aperçu plein écran ouvert est d'abord fermé.

## 9. Import d'images sans clavier ni souris

- **Lot 2** : boîte de dialogue du système (`rfd::FileDialog`).
- **Lot 4** : explorateur intégré, tactile, limité aux supports amovibles et au dossier
  Images (Windows : lecteurs amovibles ; macOS : `/Volumes/*` ; Linux :
  `/media/$USER/*`, `/run/media/$USER/*`), fichiers `.png` / `.jpg` / `.jpeg` / `.zip`,
  lignes de 64 px, vignettes chargées par le fil de travail (12 à la fois). Composant
  partagé (`src/kiosk/admin/file_picker.rs`) avec l'import de cadre et de thème ZIP.

## 10. Lots

Chaque lot : tests, clippy sans avertissement, scénario Xvfb avec captures regardées,
mesure CPU / RAM, statut mis à jour ici.

| Lot | Contenu | Effort | Dépend de |
| --- | --- | --- | --- |
| T1.1 — Socle | `fonts.rs`, schéma 2 + `Serialize` + `write_manifest`, `theme_store`, `theme_design` (`render_background`, `prepare_import`, `contrast`), `Palette::from_colors`. Rien de visible. | M | M1.2 (`backups`) |
| T1.2 — Éditeur | Page Thème revue, éditeur (Identité, Couleurs avec sélecteur et palettes, Accueil, Textes), aperçu simplifié, Enregistrer / Appliquer, fenêtre « Quitter sans enregistrer ? », fil `bs-studio`, import par `rfd`. | L | T1.1, M1.1 |
| T1.3 — Aperçu fidèle, historique, export | Aperçu plein écran, onglet Historique (restaurer), `export_zip`. | M | T1.2 |
| T1.4 — Explorateur tactile | § 9. | M | T1.2 |

Ordre conseillé de l'ensemble : **M1.1 → F1.1 → F1.2 → F1.3 → K1 → M1.2 → T1.1 → T1.2 →
F1.4 → T1.3 → T1.4 → M1.3** (les cadres par événement apportent plus de valeur que
l'éditeur de thèmes et passent d'abord).

## 11. Tests

| Module | Cas |
| --- | --- |
| `theme_config` | Schémas 1 et 2 acceptés, 3 refusé ; thème sans `frame` accepté ; `[design]` avec couleur invalide refusé ; `write_manifest` → `load` identique ; `staging-*` ignoré par `discover`. |
| `theme_store` | `free_id` (accents, collision avec un thème livré, 48 caractères) ; `duplicate` d'un thème livré ; `commit` remplace sans laisser de dossier provisoire et crée une sauvegarde ; 4 enregistrements → 3 sauvegardes ; brouillon invalide → thème en place intact ; `delete` refuse livré et actif ; `export_zip` → `import_zip` redonne le même thème. |
| `theme_design` | Dégradé : couleurs exactes en haut et en bas ; import JPEG et PNG ; image 8000×8000 refusée avant décodage ; fichier tronqué → erreur sans panique ; `contrast` noir / blanc = 21, identiques = 1. |

Scénarios Xvfb :

1. Nouveau thème → changer l'accent → Enregistrer et appliquer → Retour aux photos : le
   live utilise la nouvelle couleur.
2. Modifier un thème livré : la copie « (personnalisé) » apparaît, l'original est intact.
3. Modifier une couleur puis « ← Thèmes » : la fenêtre « Quitter sans enregistrer ? »
   apparaît ; vérifier les trois boutons.
4. Trois enregistrements successifs puis Restaurer la plus ancienne : couleurs d'origine.
5. Ouvrir puis fermer sans enregistrer : aucun dossier `staging-*`, RAM revenue.
6. Aperçu plein écran puis fermeture : thème actif d'origine revenu ; le cadre de
   l'événement en cours n'a pas changé.

## 12. Critères d'acceptation

- [ ] Créer un thème complet sans quitter l'application, l'appliquer : écrans invités
      et maintenance prennent ses couleurs, son logo, ses textes.
- [ ] Changer de thème **ne change pas** le cadre des tirages.
- [ ] Modifier un thème livré crée une copie ; l'original reste disponible et inchangé.
- [ ] Un seul thème livré : Party. Aucun thème « Mariage », « Anniversaire » ni de marque dans le dépôt.
- [ ] Une installation neuve, sans aucun thème d'utilisateur, fonctionne entièrement avec Party.
- [ ] Sortir avec des modifications demande toujours confirmation, avec le choix
      d'enregistrer, de quitter sans enregistrer ou de rester.
- [ ] Trois versions précédentes restaurables par thème.
- [ ] Un échec d'enregistrement ou une coupure de courant ne casse jamais le thème en place.
- [ ] Le ZIP exporté s'importe sur une autre installation.
- [ ] Éditeur ouvert et immobile : 0 % de CPU ; RAM ≤ +15 Mo ; aucune opération d'image
      sur le fil d'interface.
- [ ] Tout est faisable au doigt (avec le clavier tactile pour les textes).
- [ ] Tests et clippy passent ; `docs/ARCHITECTURE.md` § 7 mis à jour.
