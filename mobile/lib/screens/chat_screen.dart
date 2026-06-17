// ChatScreen: first-class chat UI (D-06).
// Messages sent via ApiService.sendMessage(); SSE events update UI in real-time.
// BottomNavigationBar navigates to CockpitScreen.
import 'package:flutter/material.dart';
import 'dart:convert';
import '../services/api_service.dart';
import '../services/sse_service.dart';
import 'cockpit_screen.dart';
import 'pairing_screen.dart';

class ChatMessage {
  final String content;
  final bool isUser;
  final DateTime timestamp;
  ChatMessage({required this.content, required this.isUser}) : timestamp = DateTime.now();
}

class ChatScreen extends StatefulWidget {
  final ApiService api;
  const ChatScreen({super.key, required this.api});

  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends State<ChatScreen> {
  final _messages = <ChatMessage>[];
  final _inputCtrl = TextEditingController();
  final _scroll = ScrollController();
  final _sse = SseService();
  bool _sending = false;
  int _navIndex = 0;

  @override
  void initState() {
    super.initState();
    _startSse();
  }

  Future<void> _startSse() async {
    final url = await widget.api.getDaemonUrl();
    _sse.start(
      daemonUrl: url,
      onEvent: (event) {
        // Parse SEAM #4 OTel broadcast events
        try {
          final data = jsonDecode(event) as Map<String, dynamic>;
          final type = data['type'] as String? ?? 'event';
          if (type != 'mesh_sync') {
            setState(() => _messages.add(ChatMessage(content: '[event] $event', isUser: false)));
          }
        } catch (_) {}
      },
      onAuthExpired: () {
        widget.api.clearAuth();
        if (mounted) {
          Navigator.of(context).pushReplacement(
            MaterialPageRoute(builder: (_) => PairingScreen(api: widget.api, onPaired: () {})),
          );
        }
      },
    );
  }

  Future<void> _send() async {
    final text = _inputCtrl.text.trim();
    if (text.isEmpty || _sending) return;
    _inputCtrl.clear();
    setState(() {
      _messages.add(ChatMessage(content: text, isUser: true));
      _sending = true;
    });
    try {
      final reply = await widget.api.sendMessage(text);
      setState(() => _messages.add(ChatMessage(content: reply, isUser: false)));
    } catch (e) {
      setState(() => _messages.add(ChatMessage(content: 'Error: $e', isUser: false)));
    } finally {
      if (mounted) {
        setState(() => _sending = false);
        if (_scroll.hasClients) {
          _scroll.animateTo(
            _scroll.position.maxScrollExtent,
            duration: const Duration(milliseconds: 300),
            curve: Curves.easeOut,
          );
        }
      }
    }
  }

  @override
  void dispose() {
    _sse.dispose();
    _inputCtrl.dispose();
    _scroll.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (_navIndex == 1) {
      return CockpitScreen(api: widget.api, onBack: () => setState(() => _navIndex = 0));
    }
    return Scaffold(
      appBar: AppBar(title: const Text('Bastion')),
      body: Column(
        children: [
          Expanded(
            child: ListView.builder(
              controller: _scroll,
              itemCount: _messages.length,
              itemBuilder: (ctx, i) {
                final msg = _messages[i];
                return Align(
                  alignment: msg.isUser ? Alignment.centerRight : Alignment.centerLeft,
                  child: Container(
                    margin: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
                    padding: const EdgeInsets.all(12),
                    decoration: BoxDecoration(
                      color: msg.isUser ? Colors.deepPurple : Colors.grey[200],
                      borderRadius: BorderRadius.circular(12),
                    ),
                    child: Text(
                      msg.content,
                      style: TextStyle(color: msg.isUser ? Colors.white : Colors.black87),
                    ),
                  ),
                );
              },
            ),
          ),
          Padding(
            padding: const EdgeInsets.all(8),
            child: Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _inputCtrl,
                    decoration: const InputDecoration(hintText: 'Message...'),
                    onSubmitted: (_) => _send(),
                  ),
                ),
                IconButton(
                  icon: const Icon(Icons.send),
                  onPressed: _sending ? null : _send,
                ),
              ],
            ),
          ),
        ],
      ),
      bottomNavigationBar: BottomNavigationBar(
        currentIndex: _navIndex,
        onTap: (i) => setState(() => _navIndex = i),
        items: const [
          BottomNavigationBarItem(icon: Icon(Icons.chat), label: 'Chat'),
          BottomNavigationBarItem(icon: Icon(Icons.dashboard), label: 'Cockpit'),
        ],
      ),
    );
  }
}
