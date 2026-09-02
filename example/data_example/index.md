---
title: Massachusetts Population 65 and Older, by County
tags: [census, python, pygris, data]
---
# Massachusetts Population 65 and Older, by County

A small literate program: it fetches county geography and age data for
Massachusetts from the US Census Bureau, and computes what percentage of
each county's population is 65 years old or older. The explanation and
the code that does it live in the same file -- run `dankg eval index.md --block report` and the numbers at the bottom of this file are
reproducible from what is written above them.

Every code block below is Python, fenced `uv` rather than `python` so
`dankg eval` dispatches it through `[lang.uv]` -- `command = uv run {file}` -- rather than naming the interpreter directly; the fence tag
says how the block is run, not just what it is written in. Every block
declares its own dependencies inline too (see [Setup](#setup)), so `uv run` builds an ephemeral environment for the whole chain with no separate
project file to keep in sync.

Data comes from the [pygris](https://walker-data.com/pygris/) package
(county geography and identifiers) and the Census Bureau's own [data
API](https://www.census.gov/data/developers/data-sets.html) (the age
breakdown), joined on the county `GEOID` both agree on.

## Getting a Census API key

The Census API now rejects even small, anonymous requests to the ACS
endpoint -- a query with no key gets redirected to an error page instead of
data. A key is free and instant: sign up at
[api.census.gov/data/key_signup.html](https://api.census.gov/data/key_signup.html)
and export it before running this file:

```sh
export CENSUS_API_KEY=your-key-here
```

[Setup](#setup) reads it from the environment and refuses to go further
without it, rather than letting a confusing HTML redirect stand in for
the real error.

## Setup

The `# /// script` block below is [inline script
metadata](https://packaging.python.org/en/latest/specifications/inline-script-metadata/)
(PEP 723): `uv run` reads it straight out of the file it is given and
installs exactly these two packages into a throwaway environment before
running anything, which is what "all in uv" means here -- no
`pyproject.toml`, no `requirements.txt`, no environment the reader has to
remember to activate. It has to be the first thing in the file to be
recognised, which is also why this is the one block nothing else depends
on and everything else, transitively, does.

```uv name=setup
# /// script
# requires-python = ">=3.11"
# dependencies = ["pygris", "pandas"]
# ///
import os
import sys

import pandas as pd
from pygris import counties
from pygris.data import get_census

CENSUS_API_KEY = os.environ.get("CENSUS_API_KEY")
if not CENSUS_API_KEY:
    sys.exit(
        "CENSUS_API_KEY is not set -- see \"Getting a Census API key\" above."
    )

STATE_FIPS = "25"  # Massachusetts
YEAR = 2022  # most recent ACS 5-year estimates at the time of writing
```

## County geography

This analysis never draws a map, so the only thing pulled from `pygris`'s
geography side is the attribute table: every Massachusetts county's
`GEOID` (a 5-digit code -- 2-digit state FIPS plus 3-digit county FIPS)
and its name. `cb=True` asks for the generalized cartographic boundary
file rather than the full-resolution TIGER/Line one, which is smaller and
irrelevant either way since the geometry column itself is dropped here.

```uv name=fetch_counties deps=setup
ma_counties = counties(state="MA", cb=True, year=YEAR, cache=True)[["GEOID", "NAME"]]
```

## Age and sex data from the ACS

The Census Bureau's American Community Survey publishes age broken down
by sex in table `B01001`, not as a single "65 and over" column: reaching
that number means summing every bin from 65 up, for both sexes.

|         | 65-66 | 67-69 | 70-74 | 75-79 | 80-84 | 85+ |
|---------|-------|-------|-------|-------|-------|-----|
| Male    | `_020E` | `_021E` | `_022E` | `_023E` | `_024E` | `_025E` |
| Female  | `_044E` | `_045E` | `_046E` | `_047E` | `_048E` | `_049E` |

alongside `B01001_001E`, the table's own total population count, which is
the denominator. `get_census` is `pygris`'s thin wrapper around the raw
Census API; `return_geoid=True` is what makes its output joinable against
`fetch_counties`' own `GEOID` column above, rather than the separate
`state`/`county` code columns the API returns on its own.

```uv name=fetch_age_data deps=setup
AGE_VARS = [
    "B01001_001E",  # total population
    "B01001_020E", "B01001_021E", "B01001_022E", "B01001_023E", "B01001_024E", "B01001_025E",  # male, 65 and over
    "B01001_044E", "B01001_045E", "B01001_046E", "B01001_047E", "B01001_048E", "B01001_049E",  # female, 65 and over
]

age_data = get_census(
    dataset="acs/acs5",
    variables=AGE_VARS,
    year=YEAR,
    params={"for": "county:*", "in": f"state:{STATE_FIPS}", "key": CENSUS_API_KEY},
    return_geoid=True,
    guess_dtypes=True,
)
```

## Computing the percentage

Joining the two tables on `GEOID` lines up each county's name with its own
age counts; from there the percentage is exactly what it sounds like --
the twelve 65-and-over bins, summed, over the table's own total. This
block depends on both [County geography](#county-geography) and [Age and
sex data from the ACS](#age-and-sex-data-from-the-acs) even though neither
is its own heading's ancestor -- `deps=` resolves against the whole file,
not just a block's own section.

```uv name=compute deps=fetch_counties,fetch_age_data
SENIOR_VARS = [v for v in AGE_VARS if v != "B01001_001E"]

report = ma_counties.merge(age_data, on="GEOID")
report["population_65_plus"] = report[SENIOR_VARS].sum(axis=1)
report["pct_65_plus"] = (report["population_65_plus"] / report["B01001_001E"] * 100).round(1)

report = (
    report.rename(columns={"NAME": "county", "B01001_001E": "total_population"})
    [["county", "total_population", "population_65_plus", "pct_65_plus"]]
    .sort_values("pct_65_plus", ascending=False)
    .reset_index(drop=True)
)
```

## Report

The only block that produces output DanKG will write back into this
file. Running `dankg eval index.md --block report` plans and runs this
block plus its whole chain -- `setup`, `fetch_counties`,
`fetch_age_data`, `compute`, in that order -- as the one concatenated
script `uv run` actually executes.

```uv name=report deps=compute
print(report.to_string(index=False))
```
