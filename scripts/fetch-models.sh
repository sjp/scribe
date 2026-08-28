#!/bin/sh
#
# Downloads the OCR models into `models/`, which is not tracked by git.
#
# The library never fetches anything itself: callers hand it model bytes. This
# script is a convenience for running the tools and the tests in this
# repository against the models the engine was trained with.

set -eu

base_url="https://ocrs-models.s3-accelerate.amazonaws.com"
directory="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)/models"

mkdir -p "$directory"
for model in text-detection text-recognition; do
    echo "Fetching $model.rten"
    curl --fail --location --progress-bar \
        "$base_url/$model.rten" \
        --output "$directory/$model.rten"
done

cat <<EOF

The models are in $directory. To run the tests that need them:

    SCRIBE_DETECTION_MODEL=$directory/text-detection.rten \\
    SCRIBE_RECOGNITION_MODEL=$directory/text-recognition.rten \\
    cargo test --workspace
EOF
