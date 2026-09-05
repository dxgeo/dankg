# Reader

Different directory, different relative spelling of the same
repo-relative artifact `data/out.csv` that [prod.md](../prod.md) writes.

```sh name=reader deps=../../cross/prod.md#producer reads=file:../../data/out.csv
cat ../../data/out.csv
```
