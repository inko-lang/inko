#!/usr/bin/env bash

set -e

LLVM_VERSION='15'

function apt_install_llvm {
    curl \
        --retry 10 --retry-connrefused --silent --show-error --fail --location \
        https://apt.llvm.org/llvm-snapshot.gpg.key | \
        tee /etc/apt/trusted.gpg.d/apt.llvm.org.asc

    add-apt-repository \
        "deb http://apt.llvm.org/${1}/ llvm-toolchain-${1}-${LLVM_VERSION} main"
    apt-get update --yes
    apt-get install --yes llvm-${LLVM_VERSION} llvm-${LLVM_VERSION}-dev \
        libclang-common-${LLVM_VERSION}-dev libpolly-${LLVM_VERSION}-dev
}

echo "::group::Installing dependencies"

if [ "${1}" = "ubuntu:latest" ]
then
    apt-get update --yes
    apt-get install --yes llvm-${LLVM_VERSION} llvm-${LLVM_VERSION}-dev \
        libstdc++-11-dev libclang-common-${LLVM_VERSION}-dev zlib1g-dev curl \
        build-essential git

elif [ "${1}" = "ubuntu:20.04" ]
then
    apt-get update --yes
    apt-get install --yes libstdc++-10-dev zlib1g-dev curl build-essential \
        software-properties-common git
    apt_install_llvm focal

elif [ "${1}" = "debian:latest" ]
then
    apt-get update --yes
    apt-get install --yes llvm-${LLVM_VERSION} llvm-${LLVM_VERSION}-dev \
        libstdc++-11-dev libclang-common-${LLVM_VERSION}-dev zlib1g-dev curl \
        build-essential git

elif [ "${1}" = "debian:11" ]
then
    apt-get update --yes
    apt-get install --yes libstdc++-10-dev zlib1g-dev curl build-essential \
        software-properties-common git
    apt_install_llvm bullseye

elif [ "${1}" = "fedora:37" ]
then
    dnf install --assumeyes gcc make tar git \
        llvm llvm-devel llvm-static libstdc++-devel libstdc++-static \
        libffi-devel zlib-devel

elif [ "${1}" = "fedora:latest" ]
then
    dnf install --assumeyes gcc make tar git \
        llvm${LLVM_VERSION} llvm${LLVM_VERSION}-devel \
        llvm${LLVM_VERSION}-static libstdc++-devel libstdc++-static \
        libffi-devel zlib-devel

elif [ "${1}" = "archlinux:latest" ]
then
    pacman -Syu --noconfirm llvm git base-devel curl

elif [ "${1}" = "mac" ]
then
    brew install llvm@${LLVM_VERSION}
    echo "$(brew --prefix llvm@${LLVM_VERSION})/bin" >> $GITHUB_PATH
else
    echo 'An OS to install dependencies for must be specified'
    exit 1
fi

echo "::endgroup::"
