# Dashboard - Rust Metrics and Monitoring Framework

A comprehensive metrics and monitoring framework for Axum applications, providing real-time performance tracking, memory monitoring, and route analytics with a beautiful web dashboard.

## Overview

Dashboard is a powerful monitoring library designed specifically for Rust web applications built with the Axum framework. It provides detailed insights into your application's performance, memory usage, and route behavior through an intuitive web interface with real-time updates.

## Key Features

### 🚀 Real-time Monitoring
- Live system metrics (CPU, memory usage)
- Real-time route performance tracking
- WebSocket-based dashboard updates
- Process-specific monitoring

### 📊 Comprehensive Analytics
- Route call statistics with timing information
- Memory allocation and deallocation tracking
- Sliding window averages for performance trends
- Success/error rate monitoring

### 🎯 Memory Profiling
- Custom global allocator for precise tracking
- Memory delta tracking per request
- Peak memory usage monitoring
- Allocation size categorization

### 🔧 Easy Integration
- Simple middleware integration
- ~~Procedural macros for automatic instrumentation~~
- Minimal configuration required
- Non-intrusive design

### 💾 Data Persistence
- Save and load metrics data
- Historical data analysis
- Selective data deletion
- Automatic shutdown data saving

### 🎨 Interactive Dashboard
- Modern web interface
- Real-time charts and graphs
- Route blacklisting capabilities
- Responsive design

## Quick Start

### Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
dashboard = "0.1.0"
```

### Basic Usage

```rust
use axum::{Router, routing::get};
use dashboard::{create_metrics_router, auto_save_metrics_on_shutdown, instrument_routes};

#[tokio::main]
async fn main() {
    dashboard::auto_save_metrics_on_shutdown();
    
    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/api/users", get(get_users));
    
    let app = instrument_routes(app);
    
    let app = app.merge(dashboard::create_metrics_router());
    
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}


async fn get_users() -> &'static str {
    "Users endpoint"
}
```

### Accessing the Dashboard

Once your application is running, visit `http://localhost:3000/metrics` to access the monitoring dashboard.

## Architecture

The Dashboard framework consists of several key components:

### Core Library (`dashboard`)
- **Global State Management**: Thread-safe storage for metrics data
- **Memory Tracking**: Custom allocator for precise memory monitoring
- **Route Middleware**: Automatic request/response tracking
- **WebSocket Server**: Real-time data streaming to the dashboard
- **Persistence Layer**: Data saving and loading capabilities

### ~~Macro Library (`dashboard-macros`)~~
- **Route Tracking Macros**: Automatic instrumentation for individual routes
- **Middleware Macros**: Custom middleware generation
- **Bulk Instrumentation**: Automatic tracking for entire routers

### Web Dashboard
- **Real-time Interface**: Live updating charts and metrics
- **Data Visualization**: Interactive graphs using Chart.js
- **Control Panel**: Route blacklisting and data management
- **Responsive Design**: Works on desktop and mobile devices

## Workspace Structure

```
.

├── dashboard
│   └── src
│       ├── globals.rs                      # some global variables
│       ├── lib.rs                          # core library implementation
│       └── models
│           └── ...                         # models reexported in library
├── examples                                # Usage examples
│   └── basic_example                       # Basic integration example
└── ui
    └── index.html                          # Web dashboard
```

