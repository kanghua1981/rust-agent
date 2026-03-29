use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};

use crate::tools::browser::driver::{DriverError, Connection, ConnectionFactory, ConnectionConfig};

/// Connection pool for managing browser connections
pub struct ConnectionPool {
    /// Maximum number of connections
    max_connections: usize,
    
    /// Active connections
    connections: Arc<Mutex<HashMap<String, PooledConnection>>>,
    
    /// Connection semaphore for limiting concurrent connections
    semaphore: Arc<Semaphore>,
    
    /// Connection factory
    factory: Arc<dyn ConnectionFactory + Send + Sync>,
    
    /// Connection configuration
    config: ConnectionConfig,
}

/// Pooled connection
struct PooledConnection {
    /// Connection ID
    id: String,
    
    /// Actual connection
    connection: Box<dyn Connection>,
    
    /// Last time the connection was used
    last_used: Instant,
    
    /// Whether the connection is currently in use
    in_use: bool,
    
    /// Number of times this connection has been used
    use_count: u32,
    
    /// When this connection was created
    created_at: Instant,
}

/// Handle to a pooled connection
pub struct PooledConnectionHandle {
    connections: Arc<Mutex<HashMap<String, PooledConnection>>>,
    connection_id: String,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl PooledConnectionHandle {
    /// Get connection ID
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }
    
    /// Execute a function with the connection
    pub async fn with_connection<F, R>(&mut self, f: F) -> Result<R, DriverError>
    where
        F: FnOnce(&mut dyn Connection) -> R,
    {
        let mut connections = self.connections.lock().await;
        let conn = connections.get_mut(&self.connection_id)
            .ok_or_else(|| DriverError::Connection("Connection not found".to_string()))?;
        Ok(f(conn.connection.as_mut()))
    }
    
    /// Execute an async function with the connection
    pub async fn with_connection_async<F, Fut, R>(&mut self, f: F) -> Result<R, DriverError>
    where
        F: FnOnce(&mut dyn Connection) -> Fut,
        Fut: std::future::Future<Output = R>,
    {
        let mut connections = self.connections.lock().await;
        let conn = connections.get_mut(&self.connection_id)
            .ok_or_else(|| DriverError::Connection("Connection not found".to_string()))?;
        Ok(f(conn.connection.as_mut()).await)
    }
}

impl Drop for PooledConnectionHandle {
    fn drop(&mut self) {
        // Mark connection as idle when handle is dropped
        let connections = self.connections.clone();
        let connection_id = self.connection_id.clone();
        
        tokio::spawn(async move {
            let mut conns = connections.lock().await;
            if let Some(conn) = conns.get_mut(&connection_id) {
                conn.in_use = false;
                conn.last_used = Instant::now();
            }
        });
    }
}

impl ConnectionPool {
    /// Create a new connection pool
    pub fn new(
        max_connections: usize,
        factory: Box<dyn ConnectionFactory + Send + Sync>,
        config: ConnectionConfig,
    ) -> Self {
        Self {
            max_connections,
            connections: Arc::new(Mutex::new(HashMap::new())),
            semaphore: Arc::new(Semaphore::new(max_connections)),
            factory: Arc::from(factory),
            config,
        }
    }
    
    /// Get a connection from the pool
    pub async fn get_connection(&self) -> Result<PooledConnectionHandle, DriverError> {
        // Try to find an available connection
        let connection_id = self.find_available_connection_id().await;
        
        if let Some(conn_id) = connection_id {
            self.checkout_existing_connection(conn_id).await
        } else {
            self.create_new_connection().await
        }
    }
    
    /// Find an available connection ID
    async fn find_available_connection_id(&self) -> Option<String> {
        // Clean up expired connections first
        self.cleanup_expired_connections().await;
        
        let connections = self.connections.lock().await;
        for (id, connection) in connections.iter() {
            if !connection.in_use && !self.is_connection_expired(connection) {
                return Some(id.clone());
            }
        }
        
        None
    }
    
    /// Checkout an existing connection
    async fn checkout_existing_connection(&self, conn_id: String) -> Result<PooledConnectionHandle, DriverError> {
        // Acquire semaphore permit
        let permit = self.semaphore.clone().acquire_owned().await
            .map_err(|e| DriverError::Connection(format!("Failed to acquire semaphore: {}", e)))?;
        
        // Update connection state
        let mut connections = self.connections.lock().await;
        if let Some(connection) = connections.get_mut(&conn_id) {
            connection.in_use = true;
            connection.last_used = Instant::now();
            connection.use_count += 1;
            
            // Validate connection if configured
            if self.config.validate_on_checkout && !connection.connection.is_valid() {
                // Connection is invalid, remove it
                connections.remove(&conn_id);
                drop(connections);
                drop(permit);
                return self.create_new_connection().await;
            }
            
            return Ok(PooledConnectionHandle {
                connections: self.connections.clone(),
                connection_id: conn_id,
                _permit: permit,
            });
        }
        
        // Connection was removed between finding and checking out
        drop(connections);
        drop(permit);
        self.create_new_connection().await
    }
    
    /// Create a new connection
    async fn create_new_connection(&self) -> Result<PooledConnectionHandle, DriverError> {
        // Check if we can create a new connection
        {
            let connections = self.connections.lock().await;
            if connections.len() >= self.max_connections {
                return Err(DriverError::Connection("Maximum connections reached".to_string()));
            }
        }
        
        // Acquire semaphore permit
        let permit = self.semaphore.clone().acquire_owned().await
            .map_err(|e| DriverError::Connection(format!("Failed to acquire semaphore: {}", e)))?;
        
        // Create new connection
        let connection = self.factory.create_connection(&self.config)?;
        let connection_id = connection.id().to_string();
        
        let pooled_connection = PooledConnection {
            id: connection_id.clone(),
            connection,
            last_used: Instant::now(),
            in_use: true,
            use_count: 1,
            created_at: Instant::now(),
        };
        
        let mut connections = self.connections.lock().await;
        connections.insert(connection_id.clone(), pooled_connection);
        
        Ok(PooledConnectionHandle {
            connections: self.connections.clone(),
            connection_id,
            _permit: permit,
        })
    }
    
    /// Clean up expired connections
    async fn cleanup_expired_connections(&self) {
        let now = Instant::now();
        let max_age = Duration::from_secs(self.config.max_connection_age_seconds);
        let max_idle = Duration::from_secs(self.config.max_idle_time_seconds);
        
        let mut connections = self.connections.lock().await;
        connections.retain(|_, connection| {
            // Remove if too old
            if now.duration_since(connection.created_at) > max_age {
                return false;
            }
            
            // Remove if idle too long (and not in use)
            if !connection.in_use && now.duration_since(connection.last_used) > max_idle {
                return false;
            }
            
            true
        });
    }
    
    /// Check if a connection has expired
    fn is_connection_expired(&self, connection: &PooledConnection) -> bool {
        let now = Instant::now();
        let max_age = Duration::from_secs(self.config.max_connection_age_seconds);
        let max_idle = Duration::from_secs(self.config.max_idle_time_seconds);
        
        // Check if too old
        if now.duration_since(connection.created_at) > max_age {
            return true;
        }
        
        // Check if idle too long (and not in use)
        if !connection.in_use && now.duration_since(connection.last_used) > max_idle {
            return true;
        }
        
        false
    }
    
    /// Get pool statistics
    pub async fn get_statistics(&self) -> PoolStatistics {
        let connections = self.connections.lock().await;
        let total_connections = connections.len();
        let active_connections = connections.values().filter(|c| c.in_use).count();
        let idle_connections = total_connections - active_connections;
        
        let total_use_count: u32 = connections.values().map(|c| c.use_count).sum();
        let avg_use_count = if total_connections > 0 {
            total_use_count as f64 / total_connections as f64
        } else {
            0.0
        };
        
        PoolStatistics {
            total_connections,
            active_connections,
            idle_connections,
            total_use_count,
            avg_use_count,
        }
    }
}

/// Pool statistics
#[derive(Debug, Clone)]
pub struct PoolStatistics {
    /// Total number of connections
    pub total_connections: usize,
    
    /// Number of active connections
    pub active_connections: usize,
    
    /// Number of idle connections
    pub idle_connections: usize,
    
    /// Total use count across all connections
    pub total_use_count: u32,
    
    /// Average use count per connection
    pub avg_use_count: f64,
}
