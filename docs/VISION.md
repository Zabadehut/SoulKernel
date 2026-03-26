# Vision — SoulKernel

SoulKernel orchestre l’activité des OS (charge, mémoire, priorités, politiques) pour **réduire ou stabiliser la consommation électrique du PC**, en validant l’effet sur la conso réelle au besoin via un **capteur externe** (par ex. prise connectée).

- Les métriques **internes** (RAPL, compteurs OS, etc.) servent à **piloter** et à documenter le comportement machine.
- Une mesure **au tableau** (prise, pince) reste la référence pour l’**énergie réellement absorbée** côté secteur ; SoulKernel peut **fusionner** cette lecture quand tu fournis un fichier JSON à jour (voir `docs/MEROSS.md`).
