# Spécification — Système de langues (chantier L1)

Statut : **à développer**. Priorité P2. Lis d'abord `AGENTS.md`.

## 1. Objectif

BoothSoft est un projet open source : son interface ne peut pas rester en français seul.

| Langue | Code | Rôle |
| --- | --- | --- |
| Anglais | `en` | **Langue par défaut** d'une installation neuve, et langue de repli |
| Français | `fr` | Langue actuelle de toute l'interface |
| Espagnol | `es` | |
| Chinois simplifié | `zh` | Demande une police dédiée (§ 6) |

Tout ce que voit un invité ou un exploitant est traduit : écrans invités, maintenance,
messages d'erreur, cartes de guidage, textes par défaut des thèmes, dates.

### Hors périmètre

- Langues écrites de droite à gauche (arabe, hébreu) : la mise en page n'est pas prévue pour.
- Traduction automatique à l'exécution, téléchargement de langues.
- Traduction de la documentation (`docs/`), traitée à part.

## 2. Ce qui existe

- Tous les textes sont des chaînes françaises **écrites en dur** dans `src/kiosk.rs`,
  `src/kiosk/burst.rs`, `src/kiosk/admin.rs`, plus des statuts techniques en anglais
  (`self.status = "Photo ready"`…) mélangés au reste.
- Les thèmes portent des libellés d'une seule langue (`[ui]` de `config.toml`), avec des
  valeurs par défaut françaises dans `theme_config.rs::Labels`.
- Dates en français fabriquées à la main (`admin.rs` : tableaux `DAYS`, `MONTHS`).
- Pluriels faits au cas par cas (`"1 tirage"` / `"{n} tirages"`).
- Polices embarquées : Fraunces 700 et DM Sans 500 / 700 — latin seulement, **aucun
  caractère chinois**.
- Ordre de grandeur : environ 300 chaînes (à compter précisément au lot L1.1).

## 3. Décisions de conception

| # | Décision | Raison |
| --- | --- | --- |
| D1 | Pas de bibliothèque d'internationalisation : une énumération de messages et une fonction par langue, vérifiées à la compilation. | Zéro dépendance, zéro coût à l'exécution, impossible d'oublier une traduction (le compilateur refuse). |
| D2 | La langue de la borne est un **réglage** (`Settings.language`), modifiable en maintenance, appliqué immédiatement. | Simple, prévisible. |
| D3 | Option « Laisser les invités choisir » : un sélecteur de langue discret sur l'écran de veille et le live. Le choix vaut pour la séance, puis la borne revient à sa langue. | Événements internationaux, sans compliquer le cas courant. |
| D4 | Installation neuve : anglais. **Installation existante mise à jour : français conservé** (migration). | « Anglais par défaut » sans surprendre les bornes déjà en service. |
| D5 | Les textes d'un thème peuvent être fournis par langue ; un texte absent retombe sur le texte par défaut de l'application dans la langue courante. | Un thème de marque reste valable dans toutes les langues. |
| D6 | Le chinois utilise une police chargée **à la demande** depuis un fichier, jamais embarquée dans l'exécutable ni chargée si la langue n'est pas utilisée. | Une police chinoise pèse plusieurs Mo et gonfle l'atlas de polices ; les budgets de RAM s'appliquent. |
| D7 | Les textes techniques du journal restent en anglais, non traduits. | Diagnostic, recherche d'erreurs. |
| D8 | Aucune concaténation de morceaux de phrase : un message à paramètres est une fonction qui reçoit ses paramètres. | L'ordre des mots change d'une langue à l'autre. |

## 4. Architecture

### 4.1 Module `src/i18n/`

```
src/i18n/
├── mod.rs      Lang, t(), fonctions à paramètres, formats de date et de nombre
├── en.rs       pub(super) fn text(msg: Msg) -> &'static str
├── fr.rs
├── es.rs
└── zh.rs
```

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Lang { #[default] En, Fr, Es, Zh }

impl Lang {
    pub const ALL: [Lang; 4] = [Lang::En, Lang::Fr, Lang::Es, Lang::Zh];
    /// Nom de la langue dans sa propre langue : "English", "Français", "Español", "中文".
    pub fn native_name(self) -> &'static str;
    pub fn code(self) -> &'static str;      // "en", "fr", "es", "zh"
}

/// Tous les textes sans paramètre. Un variant par texte, nommé par son usage.
#[derive(Clone, Copy)]
pub enum Msg {
    ReadyFrameYourself,        // « Placez-vous dans le cadre »
    ModeSinglePhoto,           // « Une photo »
    ReviewPrintOrFinish,
    AdminSectionEvents,
    // …
}

pub fn t(lang: Lang, msg: Msg) -> &'static str {
    match lang { Lang::En => en::text(msg), Lang::Fr => fr::text(msg), Lang::Es => es::text(msg), Lang::Zh => zh::text(msg) }
}
```

Chaque fichier de langue contient un `match msg { … }` **sans branche par défaut** :
ajouter un `Msg` sans le traduire dans les quatre langues ne compile pas.

### 4.2 Messages à paramètres et pluriels

Une fonction par message, dans `mod.rs`, qui délègue à chaque langue :

```rust
pub fn prints(lang: Lang, n: u32) -> String;              // "1 print" / "3 prints" / "1 tirage" / "3 tirages" / "1 copia" / "3 张照片"
pub fn prints_left(lang: Lang, n: u32) -> String;
pub fn burst_mode(lang: Lang, n: u8) -> String;           // "Burst ×5" / "Rafale ×5" / "Ráfaga ×5" / "连拍 ×5"
pub fn burst_hint(lang: Lang, n: u8) -> String;
pub fn photo_progress(lang: Lang, current: usize, total: usize) -> String;
pub fn auto_return(lang: Lang, seconds: u64) -> String;
pub fn print_confirm(lang: Lang, n: u32) -> String;
pub fn wait_estimate(lang: Lang, prints: u32, seconds: u32) -> String;
```

Règles de pluriel : anglais, français, espagnol → deux formes (attention : en français
0 et 1 sont au singulier, en anglais et en espagnol seul 1 l'est) ; chinois → une seule
forme. Écrire une petite fonction `plural(lang, n) -> Plural { One, Other }` et l'utiliser
partout.

### 4.3 Dates, heures, nombres

| Élément | en | fr | es | zh |
| --- | --- | --- | --- | --- |
| Date longue | November 14, 2026 | 14 novembre 2026 | 14 de noviembre de 2026 | 2026年11月14日 |
| Date courte avec jour | Sat, Nov 14, 2026 | sam. 14 nov. 2026 | sáb, 14 nov 2026 | 2026年11月14日 周六 |
| Heure | 24 h partout (`18:00`) — choix volontaire pour un planning sans ambiguïté | | | |
| Séparateur de milliers | 1,200 | 1 200 | 1.200 | 1,200 |

Fonctions `date_long(lang, NaiveDate)`, `date_short(lang, NaiveDate)`, `number(lang, u32)`
dans `mod.rs`, avec des tableaux de noms de mois et de jours par langue (pas de
dépendance ; `chrono` est compilé sans ses locales). Elles remplacent `DAYS` / `MONTHS`
de `admin.rs` et servent à la variable `{date}` des cadres (`EVENT_FRAMES.md`).

### 4.4 Accès depuis l'interface

- `Kiosk::lang(&self) -> Lang` : langue choisie par l'invité pour la séance si elle
  existe, sinon `settings.language`.
- Raccourci local dans chaque fonction d'écran : `let tr = |m| i18n::t(lang, m);`.
- La maintenance utilise toujours `settings.language` (jamais la langue de l'invité).
- Les statuts techniques (`self.status = "Camera stopped"`…) deviennent une énumération
  `Status` : texte anglais pour le journal, `Msg` correspondant pour l'affichage.
  Supprimer le filtrage par préfixe de `admin.rs::render_top_bar`.

## 5. Thèmes et cadres

### 5.1 Textes des thèmes par langue

Schéma du manifeste (compatible avec l'existant) :

```toml
[ui]                          # textes sans langue : valables pour toutes (comportement actuel)
welcome_title = "Que la fête\ncommence !"

[ui.en]                       # facultatif : remplace [ui] quand la langue est l'anglais
welcome_title = "Let the party\nbegin!"
[ui.es]
welcome_title = "¡Que empiece\nla fiesta!"
```

Ordre de résolution d'un texte : `[ui.<langue>]` → `[ui]` **si** le thème déclare
`language = "<langue>"` au niveau racine et que c'est la langue courante, ou s'il ne
déclare aucune langue → texte par défaut de l'application dans la langue courante.

- Nouveau champ racine facultatif `language = "fr"` : la langue dans laquelle `[ui]`
  est rédigé. Absent = `[ui]` vaut pour toutes les langues (cas des thèmes existants).
- `shutter_label`, `retake_label`, `finish_label` deviennent **facultatifs** (défauts
  de l'application), comme les autres.
- Le thème livré « Party » déclare `language = "fr"` au départ puis fournit ses quatre
  langues au lot L1.3.
- L'éditeur de thèmes (`THEME_EDITOR.md`) gagne un sélecteur de langue dans l'onglet
  Textes (lot à ajouter à ce chantier : T1.5).

### 5.2 Cadres

Les lignes de texte d'un cadre sont saisies par l'exploitant pour un événement : elles
ne sont pas traduites. Seule la variable `{date}` suit la langue de la borne.

## 6. Polices et chinois

- **Latin étendu** : vérifier que Fraunces et DM Sans couvrent tous les caractères
  espagnols (`¡ ¿ ñ á é í ó ú ü`) — c'est attendu, à confirmer par un test qui parcourt
  toutes les chaînes `es` et vérifie la présence de chaque glyphe (`ab_glyph`).
- **Chinois** : police Noto Sans SC (licence OFL), **un seul poids** (Medium), en
  version réduite aux caractères réellement utilisés par les traductions `zh` plus les
  3 500 caractères courants (pour les noms d'événements saisis) : fichier
  `assets/fonts/NotoSansSC-subset.otf`, objectif ≤ 3 Mo. Script de réduction dans
  `scripts/fonts/subset.py` (outil de dev, `fonttools`), relancé quand les traductions changent.
- **Chargement à la demande** : le fichier est lu et ajouté aux polices d'egui
  (`FontDefinitions`, en repli derrière DM Sans et Fraunces) **uniquement** quand la
  langue chinoise est active ou proposée aux invités. Retiré quand elle ne l'est plus.
  Le fichier est livré à côté de l'exécutable (dossier `fonts/`), pas embarqué.
- **Titres** : pas d'équivalent chinois de Fraunces ; les titres chinois utilisent la
  même police que le texte, en plus grand. Accepté.
- **Budget** : RAM ≤ +12 Mo quand le chinois est actif (police + glyphes dessinés),
  0 Mo sinon. À mesurer : parcourir tous les écrans en chinois puis relever la RAM.
- **Atlas de polices** : chaque caractère chinois affiché occupe de la place. Ne jamais
  animer la taille d'un texte (règle existante) ; `BigNumeral` reste en chiffres latins.
- Clavier tactile (`TOUCH_KEYBOARD.md`) : la saisie du chinois (pinyin) est **hors
  périmètre** ; en chinois, les champs de texte acceptent le clavier latin et un clavier
  physique avec la méthode de saisie du système.

## 7. Interface

### 7.1 Maintenance

Nouvelle carte **Langue** dans Caméra & écran (ou une page « Général » si elle existe) :

| Réglage | Composant | Détail |
| --- | --- | --- |
| Langue de la borne | `segmented` à 4 choix : English · Français · Español · 中文 | Chaque nom dans sa propre langue. Appliqué immédiatement à toute l'interface (maintenance comprise), enregistré avec les réglages. |
| Laisser les invités choisir | `toggle` | Affiche le sélecteur invité (§ 7.2). |
| Langues proposées aux invités | 4 cases, visibles si l'option est active | Au moins la langue de la borne. |

Premier lancement d'une installation neuve : avant la création du code, un écran
« Choose your language » avec les quatre boutons (langue par défaut : anglais, ou la
langue du système si elle fait partie des quatre — `sys-locale` n'est pas ajouté :
lire `LANG` / `LC_ALL` sous Unix et `GetUserDefaultLocaleName` sous Windows).

### 7.2 Sélecteur invité (option)

- Écran de veille et live : pastille en haut à droite affichant le code de la langue
  courante (`FR`), 56 px. Un toucher déroule les langues proposées (noms natifs).
  Pas de drapeaux (une langue n'est pas un pays).
- Le choix s'applique tout de suite, dure jusqu'à la fin de la séance
  (`reset_session`) ou 60 s sans toucher, puis revient à la langue de la borne.
- Animation d'ouverture par `ui::appear` ; rien ne se redessine au repos.

### 7.3 Mise en page

Les textes n'ont pas la même longueur selon la langue (l'espagnol et le français sont
plus longs que l'anglais, le chinois plus court et plus haut). Vérifier chaque écran
dans les quatre langues :

- boutons : largeur calculée par `ui::button_width` (déjà le cas) — vérifier que rien
  ne dépasse de l'écran à 1280 px de large ; sinon réduire le libellé, pas la police ;
- sélecteur « Une photo / Rafale », pastilles, cartes de la maintenance (colonne de
  libellés à 52 % de la largeur : `admin.rs::row`) ;
- titres de l'écran de veille sur trois lignes au plus.

## 8. Migration

`Settings::migrate`, nouvelle valeur de `defaults_version` :

- fichier de réglages existant sans champ `language` → `language = fr` (les bornes en
  service ne changent pas de langue) ;
- pas de fichier de réglages (installation neuve) → `Lang::default()` = anglais, puis
  écran de choix du § 7.1.

Attention : avec `#[serde(default)]`, un ancien fichier donnerait l'anglais ; c'est
`migrate` (fondé sur `defaults_version`) qui doit forcer le français.

## 9. Lots

| Lot | Contenu | Effort |
| --- | --- | --- |
| L1.1 — Extraction | Module `i18n`, `Msg`, fonctions à paramètres, dates ; **toutes** les chaînes de l'interface passent par `t()` ; français seul (les trois autres langues renvoient provisoirement le français via une macro de repli, retirée au lot suivant) ; statuts techniques en énumération. Aucun changement visible. | L |
| L1.2 — Anglais et espagnol | Traductions complètes `en` et `es`, réglage de langue en maintenance, migration, écran de premier lancement, vérification de la mise en page dans les trois langues. | M |
| L1.3 — Thèmes par langue | `[ui.<langue>]`, `language`, libellés obligatoires rendus facultatifs, thème Party dans les langues disponibles. | S |
| L1.4 — Chinois | Police réduite chargée à la demande, traduction `zh`, mesures RAM, mise en page. | M |
| L1.5 — Choix par l'invité | Sélecteur du § 7.2. | S |

Qualité des traductions : l'anglais et l'espagnol peuvent être rédigés par l'agent,
puis **relus par une personne dont c'est la langue** avant publication ; le chinois
**doit** être relu par un locuteur natif (noter dans le README les langues « à relire »).
Ton : court, chaleureux, direct — comme le français actuel. Tutoiement / vouvoiement :
`vous` en français, `usted` évité en espagnol au profit de tournures impersonnelles ou
de l'impératif pluriel, formes neutres en chinois.

## 10. Tests

| Cas | Vérification |
| --- | --- |
| Exhaustivité | Garantie par la compilation (aucune branche par défaut). |
| Chaînes vides | Test : pour chaque langue et chaque `Msg`, le texte n'est pas vide. |
| Glyphes | Test : chaque caractère de chaque chaîne `en` / `fr` / `es` existe dans DM Sans ou Fraunces ; chaque caractère `zh` existe dans la police réduite. |
| Pluriels | 0, 1, 2, 5 dans chaque langue pour chaque fonction à nombre. |
| Dates | 1er janvier, 14 novembre, 29 février dans les quatre langues. |
| Migration | Ancien fichier sans `language` → français ; pas de fichier → anglais. |
| Thèmes | Résolution `[ui.es]` → `[ui]` → défaut ; thème existant inchangé. |
| Longueur | Test : aucun libellé de bouton ne dépasse 28 caractères latins (ou 12 caractères chinois). |

Scénarios Xvfb : le parcours complet (veille, live, rafale, choix, merci, maintenance)
capturé dans les quatre langues ; regarder chaque capture (débordements, texte coupé,
caractères manquants affichés en carrés).

## 11. Critères d'acceptation

- [ ] Installation neuve : anglais, avec choix de la langue au premier lancement.
- [ ] Borne existante mise à jour : toujours en français, sans action.
- [ ] Les quatre langues couvrent 100 % de l'interface invité et de la maintenance ;
      aucun texte en dur ne reste dans `src/kiosk*` (recherche de chaînes accentuées
      hors `i18n/` : zéro résultat).
- [ ] Changer de langue en maintenance s'applique immédiatement, sans redémarrage.
- [ ] Chinois : aucun carré à la place d'un caractère ; RAM ≤ +12 Mo quand il est
      actif, inchangée sinon ; exécutable pas plus gros de plus de 100 Ko.
- [ ] 0 % de CPU au repos dans toutes les langues ; aucun bouton hors de l'écran.
- [ ] Un thème sans textes traduits reste utilisable dans toutes les langues.
- [ ] Tests et clippy passent ; README mis à jour (langues disponibles, langues à relire).
