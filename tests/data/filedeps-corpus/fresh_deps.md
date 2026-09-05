# Fresh via deps

```sh name=writer produces=file:out.csv
echo out > out.csv
```

```sh name=reader deps=writer reads=file:out.csv
cat out.csv
```
