# A match among several dependencies is enough

```sh name=other
echo unrelated
```

```sh name=writer produces=file:out2.csv
echo out2 > out2.csv
```

```sh name=reader deps=other,writer reads=file:out2.csv
cat out2.csv
```
