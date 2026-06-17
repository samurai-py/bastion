// CockpitScreen: D-06 (LOCKED) full cockpit panel.
//
// Four sections (all required per D-06):
// 1. Goals — conversational "/goals" via POST /webhook
// 2. DriftIndicator — reads drift state via "/drift" command; shows current drift status
// 3. ContestableMemoryView — lists beliefs via "/memories"; [contestar] per item calls "/contest <id>"
//    — reuses the EXISTING contest command path; no new daemon endpoint invented
// 4. Mesh Status — static placeholder (no /cockpit/status endpoint in this phase)

import 'package:flutter/material.dart';
import '../services/api_service.dart';

// ── DriftIndicator ────────────────────────────────────────────────────────────

class DriftIndicator extends StatefulWidget {
  final ApiService api;
  const DriftIndicator({super.key, required this.api});

  @override
  State<DriftIndicator> createState() => _DriftIndicatorState();
}

class _DriftIndicatorState extends State<DriftIndicator> {
  String _driftText = 'Loading...';
  bool _loading = false;

  @override
  void initState() {
    super.initState();
    _loadDrift();
  }

  Future<void> _loadDrift() async {
    setState(() => _loading = true);
    try {
      // Uses existing GOAL engine drift reporting via conversational /drift path
      final result = await widget.api.sendMessage('/drift');
      setState(() => _driftText = result.isNotEmpty ? result : 'No drift detected.');
    } catch (e) {
      setState(() => _driftText = 'Could not load drift: $e');
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            const Text('Drift', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 16)),
            const SizedBox(width: 8),
            IconButton(
              icon: const Icon(Icons.refresh, size: 18),
              onPressed: _loading ? null : _loadDrift,
              tooltip: 'Refresh drift',
            ),
          ],
        ),
        const SizedBox(height: 4),
        _loading
            ? const SizedBox(height: 24, child: Center(child: CircularProgressIndicator(strokeWidth: 2)))
            : Text(_driftText, style: const TextStyle(color: Colors.black87)),
      ],
    );
  }
}

// ── ContestableMemoryView ─────────────────────────────────────────────────────

/// A single belief entry parsed from the "/memories" response.
/// Expected format: lines like "42: I prefer coffee in the morning"
class BeliefEntry {
  final String id;
  final String content;
  BeliefEntry({required this.id, required this.content});

  static List<BeliefEntry> parseFromResponse(String response) {
    return response
        .split('\n')
        .map((line) => line.trim())
        .where((line) => line.isNotEmpty)
        .map((line) {
          final colonIdx = line.indexOf(':');
          if (colonIdx < 1) return null;
          final id = line.substring(0, colonIdx).trim();
          final content = line.substring(colonIdx + 1).trim();
          if (id.isEmpty || content.isEmpty) return null;
          return BeliefEntry(id: id, content: content);
        })
        .whereType<BeliefEntry>()
        .toList();
  }
}

class ContestableMemoryView extends StatefulWidget {
  final ApiService api;
  const ContestableMemoryView({super.key, required this.api});

  @override
  State<ContestableMemoryView> createState() => _ContestableMemoryViewState();
}

class _ContestableMemoryViewState extends State<ContestableMemoryView> {
  List<BeliefEntry> _beliefs = [];
  bool _loading = false;
  String? _error;
  String? _contestingId;

  @override
  void initState() {
    super.initState();
    _loadMemories();
  }

  Future<void> _loadMemories() async {
    setState(() { _loading = true; _error = null; });
    try {
      final result = await widget.api.sendMessage('/memories');
      setState(() => _beliefs = BeliefEntry.parseFromResponse(result));
    } catch (e) {
      setState(() => _error = 'Could not load memories: $e');
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  /// Contest a belief by ID using the existing contest command path.
  /// Calls "/contest <id>" — reuses the daemon's existing contest skill.
  Future<void> _contest(String beliefId) async {
    setState(() => _contestingId = beliefId);
    try {
      final result = await widget.api.sendMessage('/contest $beliefId');
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(result.isNotEmpty ? result : 'Contestado.')),
        );
        await _loadMemories(); // refresh after contest
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Erro ao contestar: $e')),
        );
      }
    } finally {
      if (mounted) setState(() => _contestingId = null);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            const Text('Memoria Contestavel', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 16)),
            const SizedBox(width: 8),
            IconButton(
              icon: const Icon(Icons.refresh, size: 18),
              onPressed: _loading ? null : _loadMemories,
              tooltip: 'Atualizar',
            ),
          ],
        ),
        const SizedBox(height: 4),
        if (_error != null) Text(_error!, style: const TextStyle(color: Colors.red)),
        if (_loading && _beliefs.isEmpty)
          const SizedBox(height: 32, child: Center(child: CircularProgressIndicator(strokeWidth: 2)))
        else if (_beliefs.isEmpty && !_loading)
          const Text('Nenhuma memoria disponivel.', style: TextStyle(color: Colors.grey))
        else
          ..._beliefs.map((belief) => Padding(
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Row(
              children: [
                Expanded(
                  child: Text('${belief.id}: ${belief.content}',
                      style: const TextStyle(fontSize: 13)),
                ),
                const SizedBox(width: 8),
                TextButton(
                  onPressed: _contestingId == belief.id ? null : () => _contest(belief.id),
                  style: TextButton.styleFrom(
                    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                    foregroundColor: Colors.deepPurple,
                  ),
                  child: _contestingId == belief.id
                      ? const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2))
                      : const Text('[contestar]', style: TextStyle(fontSize: 12)),
                ),
              ],
            ),
          )),
      ],
    );
  }
}

// ── CockpitScreen ─────────────────────────────────────────────────────────────

class CockpitScreen extends StatefulWidget {
  final ApiService api;
  final VoidCallback onBack;
  const CockpitScreen({super.key, required this.api, required this.onBack});

  @override
  State<CockpitScreen> createState() => _CockpitScreenState();
}

class _CockpitScreenState extends State<CockpitScreen> {
  // Goals loaded conversationally via POST /webhook with message="/goals".
  String _goalsText = 'Loading...';
  bool _loadingGoals = false;

  @override
  void initState() {
    super.initState();
    _loadGoals();
  }

  Future<void> _loadGoals() async {
    setState(() => _loadingGoals = true);
    try {
      final goals = await widget.api.sendMessage('/goals');
      setState(() => _goalsText = goals.isNotEmpty ? goals : 'No active goals.');
    } catch (e) {
      setState(() => _goalsText = 'Could not load goals: $e');
    } finally {
      if (mounted) setState(() => _loadingGoals = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Cockpit'),
        leading: IconButton(icon: const Icon(Icons.arrow_back), onPressed: widget.onBack),
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            onPressed: _loadingGoals ? null : _loadGoals,
            tooltip: 'Refresh goals',
          ),
        ],
      ),
      body: RefreshIndicator(
        onRefresh: _loadGoals,
        child: ListView(
          padding: const EdgeInsets.all(16),
          children: [
            // 1. Goals (D-06)
            const Text('Goals', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 18)),
            const SizedBox(height: 8),
            _loadingGoals
                ? const Center(child: CircularProgressIndicator())
                : Text(_goalsText),
            const Divider(height: 32),

            // 2. Drift (D-06) — reads drift state from GOAL engine via /drift command
            DriftIndicator(api: widget.api),
            const Divider(height: 32),

            // 3. Contestable Memory (D-06) — lists beliefs; [contestar] calls /contest <id>
            ContestableMemoryView(api: widget.api),
            const Divider(height: 32),

            // 4. Mesh Status (D-06) — static placeholder MVP (no /cockpit/status endpoint in this phase)
            const Text('Mesh Status', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 18)),
            const SizedBox(height: 8),
            const Text(
              'To connect mesh peers: type /connect-peer in Bastion chat.\n'
              'Active peers and sync status will appear here once connected.',
              style: TextStyle(color: Colors.grey),
            ),
          ],
        ),
      ),
    );
  }
}
