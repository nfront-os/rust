// C++ against APE in a Rust program: libstdc++'s threads on APE's pthreads (on std's threads).
#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstdio>
#include <mutex>
#include <shared_mutex>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>

static thread_local int tl = 0;
static std::once_flag once;
static int oncecount;

extern "C" int
cxx_test(void)
{
	std::mutex m;
	std::condition_variable cv;
	long sum = 0;
	std::vector<std::thread> ts;
	for (int i = 0; i < 8; i++)
		ts.emplace_back([&, i] {
			tl = i + 1;
			std::call_once(once, [] { oncecount++; });
			for (int j = 0; j < 1000; j++) {
				std::lock_guard<std::mutex> g(m);
				sum += tl;
			}
		});
	for (auto &t : ts)
		t.join();
	if (sum != 36000 || oncecount != 1 || tl != 0)
		return 1;
	// a condition, with a timeout and with a notify
	std::unique_lock<std::mutex> l(m);
	if (cv.wait_for(l, std::chrono::milliseconds(50)) != std::cv_status::timeout)
		return 2;
	bool ready = false;
	std::thread n([&] {
		std::this_thread::sleep_for(std::chrono::milliseconds(20));
		std::lock_guard<std::mutex> g(m);
		ready = true;
		cv.notify_one();
	});
	cv.wait(l, [&] { return ready; });
	l.unlock();
	n.join();
	std::shared_mutex sm;
	{ std::shared_lock<std::shared_mutex> r1(sm); std::shared_lock<std::shared_mutex> r2(sm); }
	{ std::unique_lock<std::shared_mutex> w(sm); }
	try {
		throw std::runtime_error("expected");
	} catch (const std::exception &e) {
		if (std::string(e.what()) != "expected")
			return 3;
	}
	std::printf("cxx: ok\n");
	std::fflush(stdout);
	return 0;
}
