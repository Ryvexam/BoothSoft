# Spécification — Clavier tactile de la maintenance (chantier K1)

Statut : **à développer**. Prérequis pour saisir du texte sur la borne (écran tactile
15,6", ni clavier ni souris) : nom d'événement, nom de thème, textes du thème, code
couleur. Lis d'abord `AGENTS.md`.

## 1. Constat

La maintenance contient des champs `egui::TextEdit` (nom d'événement dans
`admin.rs::page_event`, nouveau code PIN dans `page_security`, bientôt tous les champs
de l'éditeur de thèmes). Sur la borne, rien ne permet d'y écrire : le clavier tactile
de Windows n'apparaît pas de façon fiable par-dessus une application plein écran, et il
sortirait l'utilisateur du mode borne.

Le code PIN d'entrée a déjà son pavé numérique (`admin.rs::render_pin`) : s'en inspirer.

## 2. Objectif

Un clavier dessiné par l'application, qui apparaît en bas de l'écran dès qu'un champ de
texte de la maintenance reçoit le focus, et qui écrit dans ce champ. Jamais visible
côté invités.

## 3. Comportement

- **Apparition** : quand un `TextEdit` a le focus (`ctx.memory(|m| m.focused())` non vide
  et `ctx.wants_keyboard_input()`), le clavier monte du bas en 200 ms (`ui::appear`).
  Il disparaît quand le focus est perdu ou sur la touche « Terminé ».
- **Désactivation automatique** : si une frappe d'un clavier physique est reçue
  (événement `egui::Event::Key` ou `Text` qui ne vient pas du clavier tactile), le
  clavier tactile se masque pour la suite de la session de maintenance. Réglage
  « Clavier tactile : Automatique / Toujours / Jamais » dans Caméra & écran.
- **Le champ reste visible** : la zone de contenu de la maintenance est réduite de la
  hauteur du clavier, et la page défile pour garder le champ actif au-dessus
  (`response.scroll_to_me(Some(Align::Center))` à l'apparition).
- **Saisie** : chaque touche injecte un événement dans egui avant le dessin des champs :
  `ctx.input_mut(|i| i.events.push(egui::Event::Text("a".into())))`, et pour les touches
  spéciales `Event::Key { key: Backspace | Enter | ArrowLeft | ArrowRight, pressed: true, .. }`.
  Les événements doivent être poussés **au début** de l'image (avant `render_admin`),
  à partir des touchers enregistrés à l'image précédente ; sinon le champ ne les voit pas.
- **Le toucher sur une touche ne doit pas retirer le focus du champ** : dessiner le
  clavier dans une `egui::Area` au premier plan et détecter les touchers par
  `ui.interact(rect, id, Sense::click())` sans jamais appeler `request_focus` ;
  vérifier qu'egui ne retire pas le focus au clic hors du champ (sinon le redonner :
  `ctx.memory_mut(|m| m.request_focus(id_du_champ))` avec l'identifiant mémorisé).
- **Répétition** : maintenir « ⌫ » efface en continu après 400 ms (une suppression
  toutes les 60 ms). C'est la seule répétition.

## 4. Dispositions

AZERTY français, 4 rangées, hauteur totale 300 px (pour 1280×800 ; proportionnelle
ensuite), touches de 64 px de haut au moins, 6 px d'écart.

```
Lettres
┌───┬───┬───┬───┬───┬───┬───┬───┬───┬───┐
│ a │ z │ e │ r │ t │ y │ u │ i │ o │ p │
├───┼───┼───┼───┼───┼───┼───┼───┼───┼───┤
│ q │ s │ d │ f │ g │ h │ j │ k │ l │ m │
├───┴┬──┴┬──┴┬──┴┬──┴┬──┴┬──┴┬──┴┬──┴───┤
│ ⇧  │ w │ x │ c │ v │ b │ n │ ’ │  ⌫   │
├────┼───┴┬──┴───┴───┴───┴──┬┴──┬┴──────┤
│123 │ é è│      espace      │ - │Terminé│
└────┴────┴──────────────────┴───┴───────┘

Chiffres et signes (touche « 123 », retour par « abc »)
1 2 3 4 5 6 7 8 9 0
& é è ç à ù ê î ô û
. , ! ? : ; ( ) / @
abc  # { }  espace  ↵  ⌫  Terminé
```

- **⇧** : majuscule pour la lettre suivante ; double toucher = verrouillage.
  Majuscule automatique en début de champ.
- **é è** : ouvre une rangée d'accents au-dessus de la touche (é è ê ë à â ù û î ï ô ç)
  le temps d'un choix.
- **’** : apostrophe typographique.
- **↵** : seulement dans les champs multilignes (titres du thème).
- **{ }** : pour les variables `{event}` et `{date}` du cadre.

**Disposition réduite** selon le champ, choisie par un indicateur posé sur le champ
(`ui.data_mut(|d| d.insert_temp(id, KeyboardKind::Hex))` juste avant le `TextEdit`) :

| Type | Touches |
| --- | --- |
| `Text` (défaut) | Disposition complète |
| `Hex` (codes couleur) | 0–9, A–F, #, ⌫, Terminé — pavé de 4×5 |
| `Digits` (code PIN, nombres) | 0–9, ⌫, Terminé — même apparence que `render_pin` |

## 5. Rendu et performances

- Dessin direct au `Painter` (rectangles arrondis + texte), comme les boutons de
  `ui.rs` ; enfoncement animé par `ui::press`. Couleurs : `admin.rs::Pal`
  (fond `surface`, touches `bg`, touches d'action `primary`).
- Aucune texture, aucune police supplémentaire. Vérifier que les glyphes ⇧ et ⌫
  existent dans DM Sans ; sinon les dessiner en vecteurs (voir `backspace_glyph` dans
  `admin.rs`).
- Coût : 0 % au repos (le clavier affiché sans toucher ne redessine rien) ; RAM
  négligeable (pas d'état en dehors de la disposition courante et du champ actif).
- Tailles de police fixes (pas d'animation de taille : l'atlas de polices grossirait).

## 6. Fichiers

| Fichier | Changement |
| --- | --- |
| `src/kiosk/admin/keyboard.rs` (nouveau) | `pub struct TouchKeyboard { layout, shift, pending: Vec<egui::Event>, hidden_by_hardware: bool, target: Option<egui::Id> }` ; `fn begin_frame(&mut self, ctx)` (injecte `pending`) ; `fn show(&mut self, ctx, pal) -> f32` (dessine, remplit `pending`, renvoie la hauteur occupée). |
| `src/kiosk.rs` | Champ `keyboard: TouchKeyboard` ; appeler `begin_frame` au début de `update` quand `screen == Admin`. |
| `src/kiosk/admin.rs` | Réduire la zone de page de la hauteur renvoyée par `show` ; indicateur `KeyboardKind` sur les champs ; réglage Automatique / Toujours / Jamais. |
| `src/model.rs` | `Settings.touch_keyboard: u8` (0 auto, 1 toujours, 2 jamais), défaut 0. |

## 7. Tests et recette

- Test unitaire : séquence de touches → suite d'événements attendue (lettre, majuscule
  unique, verrouillage, accent, effacement, variable `{event}`).
- Scénario Xvfb : maintenance → Événements → toucher le champ du nom → taper « Léa »
  au clavier tactile → Terminé → le champ contient « Léa », la barre « Modifications
  non enregistrées » apparaît.
- Recette sur la borne : saisir « Mariage Léa & Hugo — 14 juin » sans clavier
  physique en moins de 40 s ; le champ reste visible pendant toute la saisie ;
  brancher un clavier physique et taper : le clavier tactile disparaît.

## 8. Effort

M (1 à 2 jours). Aucune dépendance. Débloque la saisie de texte de l'éditeur de thèmes
(`docs/specs/THEME_EDITOR.md`) et du planning d'événements sur la borne.
