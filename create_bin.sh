#!/bin/bash -e

if [ $# -ne 1 ] || [[ ! "$1" =~ ^(debug|release)$ ]]; then
    echo "Usage: $0 [debug|release]"
    exit 1
fi

BUILD_TYPE=$1

if [ "$BUILD_TYPE" == "debug" ]; then
    echo "Building debug image"
    cargo build --features=device_nvs
elif [ "$BUILD_TYPE" == "release" ]; then
    echo "Building release image"
    cargo build --release --features=device_nvs
fi

echo "Generating new partition table"
espflash partition-table my_partitions.csv -o part.bin --to-binary

echo "Original $BUILD_TYPE partition table"
espflash partition-table "/home/esp/rs_coffee/target/xtensa-esp32s3-espidf/$BUILD_TYPE/partition-table.bin"

echo "Replacing $BUILD_TYPE partition table"
cp part.bin "/home/esp/rs_coffee/target/xtensa-esp32s3-espidf/$BUILD_TYPE/partition-table.bin"

rm part.bin

echo "New partition table"
espflash partition-table "/home/esp/rs_coffee/target/xtensa-esp32s3-espidf/$BUILD_TYPE/partition-table.bin"

echo "Merging $BUILD_TYPE image"

espflash save-image \
    --flash-size 16mb \
    --chip esp32s3 \
    --merge \
    --partition-table "target/xtensa-esp32s3-espidf/$BUILD_TYPE/partition-table.bin" \
    --bootloader "target/xtensa-esp32s3-espidf/$BUILD_TYPE/bootloader.bin" \
    "target/xtensa-esp32s3-espidf/$BUILD_TYPE/rs-coffee" \
    "rs_coffee_$BUILD_TYPE.bin"