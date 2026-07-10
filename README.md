# Analyseur de fiche de paie

<p align="center">
  <img alt="Rust" src="https://img.shields.io/badge/-Rust-000000?logo=rust&logoColor=white">
  <img alt="Claude Code" src="https://img.shields.io/badge/-Claude%20Code-D97757?logo=claude&logoColor=white">
  <a href="https://github.com/bl3tt3r/analyseur-de-fiche-de-paie/actions/workflows/rust.yml"><img alt="Rust CI" src="https://github.com/bl3tt3r/analyseur-de-fiche-de-paie/actions/workflows/rust.yml/badge.svg"></a>
  <a href="https://github.com/bl3tt3r/analyseur-de-fiche-de-paie/actions/workflows/release.yml"><img alt="Release" src="https://github.com/bl3tt3r/analyseur-de-fiche-de-paie/actions/workflows/release.yml/badge.svg"></a>
</p>

**Analyseur de fiche de paie** est une application de bureau qui automatise le suivi de vos fiches de paie grâce à l'IA, notamment [Claude Code](https://docs.anthropic.com/en/docs/claude-code) : elle analyse et extrait les données de chaque PDF (salaire net, cotisations, primes...) et permet de les comparer d'un mois sur l'autre.

Initialement développée pour mon usage personnel, l'application se veut volontairement simple et se concentre sur ce périmètre, sans ambition de s'étendre au-delà.

## Fonctionnement

<img alt="Claude Code" src="https://img.shields.io/badge/Propuls%C3%A9%20par-Claude%20Code-D97757?logo=claude&logoColor=white">

L'application s'appuie sur [Claude Code](https://docs.anthropic.com/en/docs/claude-code) pour lire et analyser chaque fiche de paie (PDF) : le fichier lui est transmis avec des consignes précises, et le résultat structuré (date de paiement, salaire net, cotisations, primes, etc.) est renvoyé et stocké localement.

Claude Code doit donc être installé et connecté sur la machine pour que l'analyse fonctionne — l'application vérifie cette condition au démarrage.

Une fois les fiches analysées, les données sont affichées sous forme de graphique, avec la possibilité de choisir les valeurs à comparer.

## Aperçu

![Image de l'application](docs/app.png)


## Prérequis

- [Claude Code](https://docs.anthropic.com/en/docs/claude-code) installé et connecté
- Rust (édition 2024), si vous compilez depuis les sources

## Installation

Télécharger le binaire correspondant à votre plateforme depuis la [page des releases](https://github.com/bl3tt3r/analyseur-de-fiche-de-paie/releases), ou compiler depuis les sources :

```bash
git clone git@github.com:bl3tt3r/analyseur-de-fiche-de-paie.git
cd analyseur-de-fiche-de-paie
cargo run --release
```

## Utilisation

1. Lancer l'application
2. Cliquer sur **Scanner une fiche** pour importer un ou plusieurs PDF
3. Chaque fiche est analysée automatiquement en arrière-plan
4. Une fois l'analyse terminée, choisir les données à comparer via le menu **Filtres** du graphique

Les fiches importées et les données extraites sont stockées localement dans le dossier `datas/`.
