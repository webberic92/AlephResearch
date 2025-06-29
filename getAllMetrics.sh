#!/bin/bash
for dir in ORIG_N12_T256_R16 ORIG_N16_T1024_R16 ORIG_N16_T1028_R10 ORIG_N8_T128_R16 \
           RSA_N12_T256_R16 RSA_N16_T1024_R16 RSA_N16_T1028_R10 RSA_N20_T256_R16 \
           RSA_N24_T2048_R16 RSA_N32_T512_R16 RSA_N64_T256_R16 RSA_N8_T128_R16
do
  aws s3 cp s3://aleph-research/logs/$dir ./"$dir.final+metrics"/ --recursive
done
