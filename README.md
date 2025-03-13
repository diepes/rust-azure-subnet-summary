# rust-azure-subnet-summary

rust calls ```az graph query``` to get all the subnets then outputs a csv of subnets for future allocations

## Create Subnets summary

* Run rust code, filtering out warn and info log messages.

      cargo run | grep -v " INFO \| WARN " > subnets-$( date -I).csv


## Problems / TODO

* Duplicate subnets returned by azure graph query
  * TODO: debug query, currently filtering duplicates and specific subnets see src/lib.rs#de_duplicate_subnets()
  