#!/usr/bin/env bash

set -e

compiler_version=$(cat compiler/LANGUAGE_VERSION)
runtime_version=$(cat runtime/LANGUAGE_VERSION)

if [[ "$compiler_version" == "$runtime_version" ]]
then
    exit 0
else
    echo -e "\e[31mThe compiler and runtime use a different language version!\e[0m"
    echo "Compiler: $compiler_version"
    echo "Runtime:  $runtime_version"
fi
