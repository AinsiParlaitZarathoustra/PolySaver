// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import React from 'react';
import { Paper, Box, Typography } from '@mui/material';
import CloudDownloadOutlinedIcon from '@mui/icons-material/CloudDownloadOutlined';
import { useTranslation } from 'react-i18next';

export const EmptyQueue: React.FC = () => {
  const { t } = useTranslation();

  return (
    <Paper
      elevation={0}
      sx={{
        p: 6,
        textAlign: 'center',
        border: '1px dashed',
        borderColor: 'divider',
        borderRadius: 3,
        bgcolor: 'background.paper',
        mt: { xs: 2.5, sm: 3 },
      }}
    >
      <Box
        sx={{
          display: 'inline-flex',
          p: 2,
          borderRadius: '50%',
          bgcolor: 'action.hover',
          mb: 2,
        }}
      >
        <CloudDownloadOutlinedIcon sx={{ fontSize: 40, color: 'text.secondary' }} />
      </Box>
      <Typography variant="h6" fontWeight={600} gutterBottom>
        {t('queue.emptyTitle')}
      </Typography>
      <Typography variant="body2" color="text.secondary" sx={{ maxWidth: 460, mx: 'auto' }}>
        {t('queue.emptySubtitle')}
      </Typography>
    </Paper>
  );
};
