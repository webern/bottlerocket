#!/bin/bash

echo "1 stdout"
>&2 echo "2 stderr"
echo "3 stdout"
>&2 echo "4 stderr"
echo "5 stdout"
