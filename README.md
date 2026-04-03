# Hyperion: Orbital Debris Avoidance & Constellation Management

Hyperion is a high-performance, deterministic flight dynamics engine built in Rust. We designed it to autonomously manage satellite constellations, route line-of-sight communications, and prevent Kessler syndrome scenarios through real-time predictive screening and evasive maneuvering.

## Core Capabilities

* **High-Speed Spatial Screening:** We use an optimized spatial partitioning grid and Runge-Kutta 4 (RK4) integration to screen thousands of active objects against dense debris fields in milliseconds.
* **The Evasion Brain:** The system continuously evaluates telemetry to predict Conjunction Data Messages (CDMs). If a Predicted Closest Approach (PCA) drops below 100 meters, Hyperion runs trial ephemerides via the Brent method to autonomously select the safest burn vector.
* **Store-and-Forward LOS Routing:** Maneuvers commanded during a communications blackout aren't dropped. They are routed to an uplink queue and automatically transmitted the second the satellite regains Line-of-Sight (LOS) with a ground station.
* **Proactive Station-Keeping:** Dodging debris creates velocity debt. Hyperion autonomously schedules  recovery burns to nullify this debt and return the satellite to its nominal commercial slot once the threat passes.
* **Mission Audit Trail:** Every maneuver decision is permanently logged to an append-only `.jsonl`

## Tech Stack

* **Backend:** Rust, Axum, Tokio
* **Frontend:** HTML, CSS, TypeScript
* **Package Manager:** Bun, Cargo

---

## Getting Started

### Prerequisites

To run this project locally, you will need **Rust (1.94.0)** and **Bun (1.3.11)**.

If you don't have the Rust toolchain installed, use the official `rustup` installer:

> **Linux / macOS:**
> ```bash
> curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.94.0
> ```
> 
> **Windows:**
> Download and run `rustup-init.exe` from [rustup.rs](https://rustup.rs/).

If you don't have Bun installed, use the official `bun.sh` script:

> **Linux / macOS:**
> ```bash
> curl -fsSL https://bun.sh/install | bash -s -- bun-v1.3.11
> ```


### Option A: Local Build (No Docker)

**1. Clone the repository**
```bash
git clone https://github.com/RohanJ2006/Hyperion.git
cd Hyperion
```

**2. Build the Frontend**
```bash
cd frontend
bun install
bun run build
```

**3. Build the Backend & Run the server**
```bash
cd ../Backend
cargo run --release
```
The system will initialize and start listening on `http://0.0.0.0:8000`.

---

### Option B: Docker Build (Recommended)

If you prefer a containerized environment to avoid installing local toolchains, you can spin up the entire stack using Docker. 

**1. Clone the repository**
```bash
git clone https://github.com/RohanJ2006/Hyperion.git
cd Hyperion
```

**2. Build the Docker Image**
This will autonomously compile both the Rust backend and the Bun frontend.
```bash
docker build -t hyperion-app .
```

**3. Run the Container**
Boot the container and map the internal port to your local machine:
```bash
docker run -p 8000:8000 hyperion-app
```
The system will initialize and start listening on `http://0.0.0.0:8000`.
