# Dependency exists but declares no produces=

```sh name=writer
echo out > out.csv
```

```sh name=reader deps=writer reads=file:out.csv
cat out.csv
```
