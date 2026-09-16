// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import React from 'react';
import {
  AppBar,
  Toolbar,
  Typography,
  IconButton,
  Box,
  Tooltip,
} from '@mui/material';
import SettingsIcon from '@mui/icons-material/Settings';
import HelpOutlineIcon from '@mui/icons-material/HelpOutline';
import { useTranslation } from 'react-i18next';
import logoIcon from '../assets/polysaver-icon.png';

interface HeaderProps {
  onOpenSettings: () => void;
  onOpenHelp: () => void;
}

export const Header: React.FC<HeaderProps> = ({ onOpenSettings, onOpenHelp }) => {
  const { t } = useTranslation();

  return (
    <AppBar
      position="static"
      color="transparent"
      elevation={0}
      sx={{
        borderBottom: 1,
        borderColor: 'divider',
        backdropFilter: 'blur(8px)',
      }}
    >
      <Toolbar
        sx={{
          position: 'relative',
          justifyContent: 'flex-end',
          minHeight: { xs: 72, sm: 88 },
          px: { xs: 2, sm: 3 },
        }}
      >
        {/* Centered brand block: title + logo, enlarged and not interactive */}
        <Box
          sx={{
            position: 'absolute',
            left: '50%',
            transform: 'translateX(-50%)',
            display: 'flex',
            alignItems: 'center',
            gap: { xs: 1.5, sm: 2 },
            pointerEvents: 'none',
          }}
        >
          <Box
            component="img"
            src={logoIcon}
            alt={t('common.appName')}
            sx={{
              width: { xs: 44, sm: 56 },
              height: { xs: 44, sm: 56 },
              objectFit: 'contain',
              display: 'block',
            }}
          />
          <Typography
            component="div"
            sx={{
              fontWeight: 800,
              letterSpacing: '-0.02em',
              fontSize: { xs: '1.6rem', sm: '2.125rem' },
              lineHeight: 1.1,
              whiteSpace: 'nowrap',
            }}
          >
            {t('common.appName')}
          </Typography>
        </Box>

        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
          <Tooltip title={t('header.helpTooltip')}>
            <IconButton
              onClick={onOpenHelp}
              color="inherit"
              aria-label={t('header.helpButtonAria')}
              sx={{
                bgcolor: (theme) =>
                  theme.palette.mode === 'dark'
                    ? 'rgba(255,255,255,0.05)'
                    : 'rgba(0,0,0,0.04)',
                '&:hover': {
                  bgcolor: (theme) =>
                    theme.palette.mode === 'dark'
                      ? 'rgba(255,255,255,0.1)'
                      : 'rgba(0,0,0,0.08)',
                },
              }}
            >
              <HelpOutlineIcon />
            </IconButton>
          </Tooltip>

          <Tooltip title={t('header.preferencesTooltip')}>
            <IconButton
              onClick={onOpenSettings}
              color="inherit"
              aria-label={t('header.preferencesButtonAria')}
              sx={{
                bgcolor: (theme) =>
                  theme.palette.mode === 'dark'
                    ? 'rgba(255,255,255,0.05)'
                    : 'rgba(0,0,0,0.04)',
                '&:hover': {
                  bgcolor: (theme) =>
                    theme.palette.mode === 'dark'
                      ? 'rgba(255,255,255,0.1)'
                      : 'rgba(0,0,0,0.08)',
                },
              }}
            >
              <SettingsIcon />
            </IconButton>
          </Tooltip>
        </Box>
      </Toolbar>
    </AppBar>
  );
};
