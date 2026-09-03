# Mismatched path

```sh name=writer produces=file:out.csv
echo out > out.csv
```

```sh name=reader deps=writer reads=file:different.csv
cat different.csv
```
