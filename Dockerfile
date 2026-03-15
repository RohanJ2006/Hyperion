FROM ubuntu:22.04

RUN apt-get update && apt-get install -y \
	curl \
	build-essential \
	pkg-config \
	libssl-dev \
	git

# install rust
RUN curl https://sh.rustup.rs -sSf | bash -s -- -y

ENV PATH="/root/.cargo/bin:${PATH}"
