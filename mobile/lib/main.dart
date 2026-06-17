import 'package:flutter/material.dart';
import 'services/api_service.dart';
import 'screens/pairing_screen.dart';
import 'screens/chat_screen.dart';

void main() {
  runApp(const BastionApp());
}

class BastionApp extends StatelessWidget {
  const BastionApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Bastion',
      theme: ThemeData(colorSchemeSeed: Colors.deepPurple, useMaterial3: true),
      home: const AppRoot(),
    );
  }
}

class AppRoot extends StatefulWidget {
  const AppRoot({super.key});

  @override
  State<AppRoot> createState() => _AppRootState();
}

class _AppRootState extends State<AppRoot> {
  final ApiService _api = ApiService();
  bool? _paired;

  @override
  void initState() {
    super.initState();
    _api.isPaired().then((paired) => setState(() => _paired = paired));
  }

  @override
  Widget build(BuildContext context) {
    if (_paired == null) return const Scaffold(body: Center(child: CircularProgressIndicator()));
    if (!_paired!) return PairingScreen(api: _api, onPaired: () => setState(() => _paired = true));
    return ChatScreen(api: _api);
  }
}
