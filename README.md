## Configure using app.py
- You can set nodes, transactions, instance type, and rounds in there.

## How To Compile

- cargo build --release --target x86_64-unknown-linux-musl

## How To Push to AWS
- aws s3 cp target/x86_64-unknown-linux-musl/release/aleph_rbc s3://aleph-research/

## build script compiles and pushes Ipserver.py and aleph artifact to S3
.build.sh

## How to Deploy to AWS
- cdk deploy

## To gather metrics
- copy logs to a sandbox area and run logs_scripts/generateReport.sh
