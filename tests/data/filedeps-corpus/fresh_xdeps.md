# Fresh via xdeps, across a language boundary

```sh name=writer produces=file:out.csv
echo out > out.csv
```

```python name=reader xdeps=writer reads=file:out.csv
print(open("out.csv").read())
```
