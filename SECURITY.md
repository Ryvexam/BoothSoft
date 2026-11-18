# Sécurité

BoothSoft tourne sur des bornes laissées sans surveillance, au contact du public.
Les signalements de sécurité sont pris au sérieux.

## Signaler une faille

Ne créez **pas** de ticket public. Écrivez en privé aux mainteneurs (adresse indiquée
sur la page du dépôt, ou fonction « Report a vulnerability » de la plateforme
d'hébergement), avec :

- la version concernée et le système (Windows, macOS, Linux) ;
- les étapes pour reproduire ;
- l'effet constaté (sortie du mode borne, accès à la maintenance sans code, lecture ou
  écriture de fichiers hors du dossier de données, exécution de code, etc.).

Vous recevrez un accusé de réception sous 7 jours. Merci de nous laisser le temps de
corriger avant toute publication.

## Périmètre

Sont concernés en particulier :

- le contournement du code de la maintenance ou du mode plein écran ;
- l'import de thèmes (archives ZIP, images) : traversée de répertoires, liens
  symboliques, fichiers piégés ;
- la lecture des fichiers de données (réglages, manifestes de séance) ;
- l'envoi à l'imprimante.

## Ce que le projet garantit déjà

- Aucun port ouvert, aucune télémétrie. Une seule connexion réseau possible : l'envoi
  d'un événement au site de l'exploitant, désactivé par défaut, déclenché à la main en
  maintenance, en HTTPS, avec une clé de borne et un code à usage unique
  (`docs/specs/GALLERY_UPLOAD.md`). La clé est stockée dans `BoothSoft/upload-key`.
- Les thèmes sont des dossiers validés (noms, tailles, formats, pas de lien
  symbolique) : jamais un chemin libre ni du code exécutable.
- Le code de la maintenance est stocké haché (bcrypt) ; 5 essais puis 60 s d'attente.
- Les réglages et manifestes sont écrits de façon atomique.

## Ce que le projet ne remplace pas

Le durcissement du système (compte sans droits d'administration, mises à jour, accès
physique aux ports USB) reste à la charge de l'exploitant.
