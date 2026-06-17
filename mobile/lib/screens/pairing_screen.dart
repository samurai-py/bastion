// PairingScreen: guides user through /connect-app pairing.
// User provides daemon URL + one-time token (OTC from Bastion chat /connect-app).
// OTC exchanged for JWT via ApiService.pair() -> POST /auth/exchange.
import 'package:flutter/material.dart';
import '../services/api_service.dart';

class PairingScreen extends StatefulWidget {
  final ApiService api;
  final VoidCallback onPaired;
  const PairingScreen({super.key, required this.api, required this.onPaired});

  @override
  State<PairingScreen> createState() => _PairingScreenState();
}

class _PairingScreenState extends State<PairingScreen> {
  final _urlCtrl = TextEditingController(text: 'http://192.168.1.X:8080');
  final _otcCtrl = TextEditingController();
  bool _loading = false;
  String? _error;

  Future<void> _pair() async {
    setState(() { _loading = true; _error = null; });
    try {
      await widget.api.pair(_urlCtrl.text.trim(), _otcCtrl.text.trim());
      widget.onPaired();
    } catch (e) {
      setState(() { _error = 'Pairing failed: $e'; });
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Connect to Bastion')),
      body: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const Text('1. In Bastion, type /connect-app to get your one-time token.'),
            const SizedBox(height: 16),
            TextField(controller: _urlCtrl, decoration: const InputDecoration(labelText: 'Daemon URL')),
            const SizedBox(height: 8),
            TextField(controller: _otcCtrl, decoration: const InputDecoration(labelText: 'One-time token (BAST-XXXX)')),
            const SizedBox(height: 24),
            if (_error != null) Text(_error!, style: const TextStyle(color: Colors.red)),
            ElevatedButton(
              onPressed: _loading ? null : _pair,
              child: _loading ? const CircularProgressIndicator() : const Text('Pair'),
            ),
          ],
        ),
      ),
    );
  }
}
