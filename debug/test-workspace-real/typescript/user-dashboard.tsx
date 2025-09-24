import React, { useState, useEffect, useCallback, useMemo } from 'react';
import { User, UserService } from './user-service';

export interface UserDashboardProps {
  userId: string;
  userService: UserService;
  onUserUpdate?: (user: User) => void;
}

export interface UserStats {
  totalLogins: number;
  lastLoginDate: Date;
  activeProjects: number;
}

export const UserDashboard: React.FC<UserDashboardProps> = ({
  userId,
  userService,
  onUserUpdate
}) => {
  const [user, setUser] = useState<User | null>(null);
  const [stats, setStats] = useState<UserStats | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);

  const loadUserData = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);

      const userData = await userService.findUserById(userId);
      if (!userData) {
        setError('User not found');
        return;
      }

      setUser(userData);

      // Load user stats (simulated)
      const userStats: UserStats = {
        totalLogins: Math.floor(Math.random() * 100),
        lastLoginDate: new Date(),
        activeProjects: Math.floor(Math.random() * 10)
      };
      setStats(userStats);

    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  }, [userId, userService]);

  useEffect(() => {
    loadUserData();
  }, [loadUserData]);

  const handleUpdateProfile = useCallback(async (updates: Partial<User>) => {
    if (!user) return;

    try {
      const updatedUser = await userService.updateUser(user.id, updates);
      if (updatedUser) {
        setUser(updatedUser);
        onUserUpdate?.(updatedUser);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Update failed');
    }
  }, [user, userService, onUserUpdate]);

  const roleNames = useMemo(() => {
    return user?.roles.map(role => role.name).join(', ') || '';
  }, [user]);

  const hasAdminPermissions = useMemo(() => {
    if (!user) return false;
    return userService.hasPermission(user, 'admin', '*');
  }, [user, userService]);

  if (loading) {
    return <LoadingSpinner message="Loading user data..." />;
  }

  if (error) {
    return <ErrorDisplay error={error} onRetry={loadUserData} />;
  }

  if (!user) {
    return <div>User not found</div>;
  }

  return (
    <div className="user-dashboard">
      <UserProfile
        user={user}
        onUpdate={handleUpdateProfile}
        canEdit={hasAdminPermissions}
      />

      <UserStatsPanel stats={stats} />

      <div className="user-roles">
        <h3>Roles</h3>
        <span className="role-badges">{roleNames}</span>
      </div>

      {hasAdminPermissions && (
        <AdminControls
          user={user}
          onUserUpdate={handleUpdateProfile}
        />
      )}
    </div>
  );
};

interface UserProfileProps {
  user: User;
  onUpdate: (updates: Partial<User>) => Promise<void>;
  canEdit: boolean;
}

const UserProfile: React.FC<UserProfileProps> = ({ user, onUpdate, canEdit }) => {
  const [isEditing, setIsEditing] = useState(false);
  const [formData, setFormData] = useState({
    name: user.name,
    email: user.email
  });

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    await onUpdate(formData);
    setIsEditing(false);
  };

  return (
    <div className="user-profile">
      <h2>User Profile</h2>

      {isEditing ? (
        <form onSubmit={handleSubmit}>
          <input
            type="text"
            value={formData.name}
            onChange={(e) => setFormData(prev => ({ ...prev, name: e.target.value }))}
            placeholder="Name"
          />
          <input
            type="email"
            value={formData.email}
            onChange={(e) => setFormData(prev => ({ ...prev, email: e.target.value }))}
            placeholder="Email"
          />
          <button type="submit">Save</button>
          <button type="button" onClick={() => setIsEditing(false)}>Cancel</button>
        </form>
      ) : (
        <div>
          <p><strong>Name:</strong> {user.name}</p>
          <p><strong>Email:</strong> {user.email}</p>
          <p><strong>Created:</strong> {user.createdAt.toLocaleDateString()}</p>
          {canEdit && (
            <button onClick={() => setIsEditing(true)}>Edit Profile</button>
          )}
        </div>
      )}
    </div>
  );
};

interface UserStatsPanelProps {
  stats: UserStats | null;
}

const UserStatsPanel: React.FC<UserStatsPanelProps> = ({ stats }) => {
  if (!stats) return null;

  return (
    <div className="user-stats">
      <h3>User Statistics</h3>
      <div className="stats-grid">
        <div className="stat-item">
          <span className="stat-label">Total Logins</span>
          <span className="stat-value">{stats.totalLogins}</span>
        </div>
        <div className="stat-item">
          <span className="stat-label">Last Login</span>
          <span className="stat-value">{stats.lastLoginDate.toLocaleDateString()}</span>
        </div>
        <div className="stat-item">
          <span className="stat-label">Active Projects</span>
          <span className="stat-value">{stats.activeProjects}</span>
        </div>
      </div>
    </div>
  );
};

interface AdminControlsProps {
  user: User;
  onUserUpdate: (updates: Partial<User>) => Promise<void>;
}

const AdminControls: React.FC<AdminControlsProps> = ({ user, onUserUpdate }) => {
  const handleDeactivate = async () => {
    if (confirm('Are you sure you want to deactivate this user?')) {
      // Implementation would go here
      console.log('Deactivating user:', user.id);
    }
  };

  return (
    <div className="admin-controls">
      <h3>Admin Controls</h3>
      <button onClick={handleDeactivate} className="danger">
        Deactivate User
      </button>
    </div>
  );
};

interface LoadingSpinnerProps {
  message?: string;
}

const LoadingSpinner: React.FC<LoadingSpinnerProps> = ({ message = 'Loading...' }) => (
  <div className="loading-spinner">
    <div className="spinner" />
    <p>{message}</p>
  </div>
);

interface ErrorDisplayProps {
  error: string;
  onRetry?: () => void;
}

const ErrorDisplay: React.FC<ErrorDisplayProps> = ({ error, onRetry }) => (
  <div className="error-display">
    <p>Error: {error}</p>
    {onRetry && <button onClick={onRetry}>Retry</button>}
  </div>
);

export default UserDashboard;