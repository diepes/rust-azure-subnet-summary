# rust-azure-subnet-summary

rust calls ```az graph query``` to get all the subnets then outputs a csv of subnets for future allocations

## Create Subnets summary

* Run rust code, filtering out warn and info log messages.

      cargo run | grep -v " INFO \| WARN " > subnets-$( date -I).csv

## Code flow

1. read subnet data for local cache or 
   1. call ```az graph query``` to get subnets in pages
   1. write to cache
1. parse pages into struct Data
1. serde parse into struct Subnet vec
1. group into struct Vnet with vec of subnets


## Problems / TODO

* Duplicate subnets returned by azure graph query
  * TODO: debug query, currently filtering duplicates and specific subnets see src/lib.rs#de_duplicate_subnets()
  