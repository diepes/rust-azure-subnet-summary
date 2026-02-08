# rust-azure-subnet-summary

rust calls ```az graph query``` to get all the subnets then outputs a csv of subnets for future allocations

## Create Subnets summary

* Run rust code, filtering out warn and info log messages.

      cargo run | grep -v " INFO \| WARN " > subnets-$( date -I).csv

## Code flow

1. read subnet data for local cache or 
   1. call ```az graph query``` to get subnets in pages
   1. write to cache (1 day)
1. parse pages into struct Data
1. serde parse into struct Subnet vec
1. group into struct Vnet with vec of subnets


## Features

* Queries Azure Resource Graph for all subnets across subscriptions
* Caches results locally (1 day TTL) to reduce API calls
* De-duplicates subnet entries (Azure Graph sometimes returns duplicates)
* Identifies gaps between allocated subnets for capacity planning
* Outputs CSV format for easy analysis in spreadsheets
* Validates subnet alignment (network address matches CIDR mask)

## Architecture

```
src/
├── main.rs           # Entry point
├── lib.rs            # Library exports
├── models/           # Data structures (Subnet, Ipv4, Vnet)
├── azure/            # Azure CLI interaction and caching
├── processing/       # De-duplication and gap finding
└── output/           # CSV and terminal formatting
```

See [ToDo.md](ToDo.md) for detailed code review notes and remaining improvements.
