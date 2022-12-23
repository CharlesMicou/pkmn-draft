#!/bin/bash

set -euo pipefail

cd js
rm -rf dist
npm run build
BUNDLE_NAME=$(ls dist | grep .js -m1)
echo $BUNDLE_NAME

cd ..

rm -rf www/static/generated
mv -f js/dist www/static/generated
mv www/static/generated/index.html www/static/generated/draft.html
