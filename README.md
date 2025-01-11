# rust-azure-subnet-summary

rust calls ```az graph query``` to get all the subnets then outputs a csv of subnets for future allocations

## Create Subnets summary

* Run rust code, filtering out warn and info log messages.

      cargo run | grep -v " INFO \| WARN " > subnets-20250111.csv
